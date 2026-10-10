//! Native menu bar and Finder Service. A menu item replays its key into egui
//! (see `take`), so the shortcut handlers stay the single source of truth.

use std::ffi::CStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use eframe::egui::{self, Key, Modifiers};
use objc2::rc::Retained;
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem, NSPasteboard, NSUpdateDynamicServices};
use objc2_foundation::{NSArray, NSBundle, NSObject, NSString, NSURL};

/// (menu, title, key equivalent, egui key). Empty menu = the app menu.
const ITEMS: &[(&str, &str, &str, Key)] = &[
    ("", "Settings…", ",", Key::Comma),
    ("Shell", "New Tab", "t", Key::T),
    ("Shell", "Split Vertically", "\\", Key::Backslash),
    ("Shell", "Split Horizontally", "|", Key::Pipe),
    ("Shell", "Close Tab", "w", Key::W),
    ("Shell", "Rename Tab", "r", Key::R),
    ("View", "Toggle Tree", "b", Key::B),
    ("View", "Focus Tree", "\u{f703}", Key::ArrowRight),
    ("View", "Unfocus Tree", "\u{f702}", Key::ArrowLeft),
    ("Editor", "Save", "s", Key::S),
    ("Editor", "Toggle Markdown Source", "e", Key::E),
    ("Editor", "Add Note", "'", Key::Quote),
    ("Editor", "Send Notes", "\r", Key::Enter),
    ("Tab", "Tab 1", "1", Key::Num1),
    ("Tab", "Tab 2", "2", Key::Num2),
    ("Tab", "Tab 3", "3", Key::Num3),
    ("Tab", "Tab 4", "4", Key::Num4),
    ("Tab", "Tab 5", "5", Key::Num5),
    ("Tab", "Tab 6", "6", Key::Num6),
    ("Tab", "Tab 7", "7", Key::Num7),
    ("Tab", "Tab 8", "8", Key::Num8),
    ("Tab", "Tab 9", "9", Key::Num9),
];

static PENDING: Mutex<Vec<Key>> = Mutex::new(Vec::new());
static PATHS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static CTX: OnceLock<egui::Context> = OnceLock::new();

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct Target;

    impl Target {
        #[unsafe(method(openInTermi:userData:error:))]
        fn open_in_termi(&self, pasteboard: &NSPasteboard, _data: Option<&NSString>, error: *mut *mut NSString) {
            match pasteboard_directories(pasteboard) {
                Ok(paths) => {
                    PATHS.lock().unwrap().extend(paths);
                    if let Some(ctx) = CTX.get() {
                        ctx.request_repaint();
                    }
                }
                Err(e) => {
                    if !error.is_null() {
                        unsafe { *error = Retained::autorelease_return(NSString::from_str(&format!("Termi: {e}"))) };
                    }
                }
            }
        }

        #[unsafe(method(fire:))]
        fn fire(&self, item: &NSMenuItem) {
            PENDING.lock().unwrap().push(ITEMS[item.tag() as usize].3);
            if let Some(ctx) = CTX.get() {
                ctx.request_repaint();
            }
        }
    }
);

/// Adds the items to winit's default menu bar. Call once the app has launched.
pub fn install(ctx: &egui::Context) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(bar) = NSApplication::sharedApplication(mtm).mainMenu() else { return };
    let _ = CTX.set(ctx.clone());
    let target: Retained<Target> = unsafe { msg_send![Target::alloc(mtm), init] };
    let mut menus: Vec<(&str, Retained<NSMenu>)> = Vec::new();
    for (i, &(menu, title, key, _)) in ITEMS.iter().enumerate() {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm), &NSString::from_str(title), Some(sel!(fire:)), &NSString::from_str(key),
            )
        };
        unsafe { item.setTarget(Some(&target)) };
        item.setTag(i as isize);
        if menu.is_empty() {
            // after About + separator
            let Some(app_menu) = bar.itemAtIndex(0).and_then(|m| m.submenu()) else { continue };
            app_menu.insertItem_atIndex(&item, 2);
            app_menu.insertItem_atIndex(&NSMenuItem::separatorItem(mtm), 3);
            continue;
        }
        if !menus.iter().any(|(m, _)| *m == menu) {
            let sub = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(menu));
            let holder = NSMenuItem::new(mtm);
            holder.setSubmenu(Some(&sub));
            bar.addItem(&holder);
            menus.push((menu, sub));
        }
        menus.iter().find(|(m, _)| *m == menu).unwrap().1.addItem(&item);
    }
    unsafe { NSApplication::sharedApplication(mtm).setServicesProvider(Some(&target)) };
    if NSBundle::mainBundle().bundleIdentifier().is_some() {
        NSUpdateDynamicServices();
    }
    // Menu items only hold their target weakly.
    std::mem::forget(target);
}

fn pasteboard_directories(pasteboard: &NSPasteboard) -> std::io::Result<Vec<PathBuf>> {
    let classes = NSArray::from_slice(&[NSURL::class()]);
    let objects = unsafe { pasteboard.readObjectsForClasses_options(&classes, None) }
        .ok_or_else(|| std::io::Error::other("select a file or folder in Finder"))?;
    let paths = objects.iter().map(|object| {
        let url = object.downcast::<NSURL>().map_err(|_| std::io::Error::other("invalid Finder URL"))?;
        if !url.isFileURL() {
            return Err(std::io::Error::other("only local files and folders can be opened"));
        }
        let bytes = unsafe { CStr::from_ptr(url.fileSystemRepresentation().as_ptr()) }.to_bytes();
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
    }).collect::<std::io::Result<Vec<_>>>()?;
    directories(paths)
}

fn directories(paths: Vec<PathBuf>) -> std::io::Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    for path in paths {
        let path = path.canonicalize()?;
        let metadata = path.metadata()?;
        let directory = if metadata.is_dir() {
            path
        } else if metadata.is_file() {
            path.parent().ok_or_else(|| std::io::Error::other("file has no parent folder"))?.to_owned()
        } else {
            return Err(std::io::Error::other("select a regular file or folder"));
        };
        if !result.contains(&directory) {
            result.push(directory);
        }
    }
    if result.is_empty() {
        return Err(std::io::Error::other("select a file or folder in Finder"));
    }
    Ok(result)
}

pub fn take_paths() -> Vec<PathBuf> {
    std::mem::take(&mut *PATHS.lock().unwrap())
}

/// Menu picks since last frame, as ⌘key press + release.
pub fn take() -> Vec<egui::Event> {
    let modifiers = Modifiers { mac_cmd: true, command: true, ..Modifiers::NONE };
    PENDING.lock().unwrap().drain(..)
        .flat_map(|key| [true, false].map(|pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finder_paths_open_folders_and_file_parents_once() {
        let root = std::env::temp_dir().join(format!("termi-finder-{}", std::process::id()));
        let folder = root.join("a ç ' $ ; folder");
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("file.txt");
        std::fs::write(&file, "test").unwrap();
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&folder, &alias).unwrap();
        assert_eq!(directories(vec![folder.clone(), file, alias, root.clone()]).unwrap(), [folder.canonicalize().unwrap(), root.canonicalize().unwrap()]);
        assert!(directories(vec![]).is_err());
        assert!(directories(vec![folder, root.join("missing")]).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_pasteboard_accepts_file_urls_and_legacy_file_lists() {
        let root = std::env::temp_dir().join(format!("termi-pasteboard-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let second = root.join("second ç folder");
        std::fs::create_dir_all(&second).unwrap();
        let path = NSString::from_str(root.to_str().unwrap());
        let second_path = NSString::from_str(second.to_str().unwrap());
        let url = NSURL::fileURLWithPath(&path);
        let pasteboard = NSPasteboard::pasteboardWithUniqueName();
        assert!(pasteboard.setString_forType(&url.absoluteString().unwrap(), &NSString::from_str("public.file-url")));
        assert_eq!(pasteboard_directories(&pasteboard).unwrap(), [root.canonicalize().unwrap()]);
        pasteboard.clearContents();
        let files = NSArray::from_slice(&[&*path, &*second_path]);
        assert!(unsafe { pasteboard.setPropertyList_forType(&files, &NSString::from_str("NSFilenamesPboardType")) });
        assert_eq!(pasteboard_directories(&pasteboard).unwrap(), [root.canonicalize().unwrap(), second.canonicalize().unwrap()]);
        pasteboard.clearContents();
        assert!(pasteboard.setString_forType(&NSString::from_str("https://example.com"), &NSString::from_str("public.url")));
        assert!(pasteboard_directories(&pasteboard).is_err());
        pasteboard.clearContents();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn take_replays_cmd_key_press_and_release() {
        PENDING.lock().unwrap().extend([Key::T, Key::W]);
        let got: Vec<(Key, bool, bool)> = take()
            .into_iter()
            .map(|e| match e {
                egui::Event::Key { key, pressed, modifiers, .. } => (key, pressed, modifiers.command && modifiers.mac_cmd),
                _ => panic!("not a key event"),
            })
            .collect();
        assert_eq!(got, [(Key::T, true, true), (Key::T, false, true), (Key::W, true, true), (Key::W, false, true)]);
        assert!(take().is_empty());
    }

    #[test]
    fn items_have_unique_shortcuts() {
        for (i, a) in ITEMS.iter().enumerate() {
            for b in &ITEMS[i + 1..] {
                assert!(a.2 != b.2 && a.3 != b.3, "{} and {} share a shortcut", a.1, b.1);
            }
        }
    }
}
