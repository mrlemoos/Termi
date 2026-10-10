//! Local notifications sent by the Termi app bundle.

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, define_class, msg_send};
use objc2_foundation::{NSBundle, NSError, NSObject, NSObjectProtocol, NSString, NSUUID};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

define_class!(
    #[unsafe(super(NSObject))]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(&self, _center: &UNUserNotificationCenter, _notification: &UNNotification, completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>) {
            completion.call((UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::List,));
        }
    }
);

fn bundled() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some_and(|id| id.to_string() == "dev.termi.app")
}

pub fn install() {
    // UNUserNotificationCenter throws an exception outside an application bundle.
    if !bundled() {
        return;
    }
    let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(), init] };
    UNUserNotificationCenter::currentNotificationCenter().setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // Notification Center holds its delegate weakly for the lifetime of the app.
    std::mem::forget(delegate);
}

fn content(title: &str, body: &str) -> Retained<UNMutableNotificationContent> {
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    content
}

pub fn notify(title: &str, body: &str) {
    if !bundled() {
        return;
    }
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSUUID::UUID().UUIDString(), &content(title, body), None,
    );
    let completion = RcBlock::new(move |granted: objc2::runtime::Bool, error: *mut NSError| {
        if let Some(error) = unsafe { error.as_ref() } {
            eprintln!("termi: notification authorisation failed: {}", error.localizedDescription());
            return;
        }
        if granted.as_bool() {
            let delivered = RcBlock::new(|error: *mut NSError| {
                if let Some(error) = unsafe { error.as_ref() } {
                    eprintln!("termi: notification failed: {}", error.localizedDescription());
                }
            });
            UNUserNotificationCenter::currentNotificationCenter().addNotificationRequest_withCompletionHandler(&request, Some(&delivered));
        }
    });
    UNUserNotificationCenter::currentNotificationCenter().requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert, &completion);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_preserves_tab_names_and_body_verbatim() {
        let title = "Termi's \"tab\" \\ ç";
        let body = "needs your input\nline two";
        let notification = content(title, body);
        assert_eq!(notification.title().to_string(), title);
        assert_eq!(notification.body().to_string(), body);
        assert!(!bundled());
        notify(title, body);
    }
}
