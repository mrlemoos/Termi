//! Native menu bar listing the hotkeys. A menu item replays its key into egui
//! (see `take`), so the shortcut handlers stay the single source of truth.

use std::sync::{Mutex, OnceLock};

use eframe::egui::{self, Key, Modifiers};
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use objc2_foundation::{NSObject, NSString};

/// (menu, title, key equivalent, egui key). Empty menu = the app menu.
const ITEMS: &[(&str, &str, &str, Key)] = &[
    ("", "Settings…", ",", Key::Comma),
    ("Shell", "New Tab", "t", Key::T),
    ("Shell", "Close Tab", "w", Key::W),
    ("Shell", "Rename Tab", "r", Key::R),
    ("View", "Toggle Tree", "b", Key::B),
    ("View", "Focus Tree", "\u{f703}", Key::ArrowRight),
    ("View", "Unfocus Tree", "\u{f702}", Key::ArrowLeft),
    ("Editor", "Save", "s", Key::S),
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
static CTX: OnceLock<egui::Context> = OnceLock::new();

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct Target;

    impl Target {
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
    // menu items only hold their target weakly
    std::mem::forget(target);
}

/// Menu picks since last frame, as ⌘key press + release.
pub fn take() -> Vec<egui::Event> {
    let modifiers = Modifiers { mac_cmd: true, command: true, ..Modifiers::NONE };
    PENDING.lock().unwrap().drain(..)
        .flat_map(|key| [true, false].map(|pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers }))
        .collect()
}
