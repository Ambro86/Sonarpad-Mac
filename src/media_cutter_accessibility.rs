//! Spoken cutter guidance without AppleScript or changes to keyboard focus.
#[cfg(target_os = "macos")]
pub(super) fn announce_voiceover_message(message: &str) {
    let message = message.trim();
    if message.is_empty() {
        return;
    }
    let message = message.to_owned();
    // AppKit requires the main thread. Defer until the button handler has updated
    // its label and focus, so those changes do not interrupt the announcement.
    wxdragon::call_after(Box::new(move || post_announcement(&message)));
}

#[cfg(target_os = "macos")]
fn post_announcement(message: &str) {
    use objc2::{MainThreadMarker, runtime::AnyObject};
    use objc2_app_kit::{
        NSAccessibilityAnnouncementKey, NSAccessibilityAnnouncementRequestedNotification,
        NSAccessibilityPostNotificationWithUserInfo, NSAccessibilityPriorityKey,
        NSAccessibilityPriorityLevel, NSApplication,
    };
    use objc2_foundation::{NSDictionary, NSNumber, NSString};

    let Some(mtm) = MainThreadMarker::new() else {
        crate::append_podcast_log("media_cutter.announcement skipped: not on main thread");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // The cutter is modal: the key window is preferable to the main window.
    let window = app.keyWindow();
    let element: &AnyObject = match window.as_ref() {
        Some(window) => window.as_ref(),
        None => app.as_ref(),
    };
    let text = NSString::from_str(message);
    let priority = NSNumber::numberWithInteger(NSAccessibilityPriorityLevel::High.0);
    // These AppKit constants are immutable. Values match their documented types:
    // NSString for the announcement and NSNumber for the priority.
    unsafe {
        let info = NSDictionary::from_slices(
            &[NSAccessibilityAnnouncementKey, NSAccessibilityPriorityKey],
            &[text.as_ref() as &AnyObject, priority.as_ref() as &AnyObject],
        );
        NSAccessibilityPostNotificationWithUserInfo(
            element,
            NSAccessibilityAnnouncementRequestedNotification,
            Some(&info),
        );
    }
    crate::append_podcast_log("media_cutter.announcement posted (AppKit)");
}

#[cfg(not(target_os = "macos"))]
pub(super) fn announce_voiceover_message(_message: &str) {}

// wx ControlDown is Command on macOS; read the physical Control modifier
// from AppKit so VoiceOver shortcuts are always left untouched.
#[cfg(target_os = "macos")]
pub(super) fn physical_control_down() -> bool {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags};
    NSEvent::modifierFlags_class().contains(NSEventModifierFlags::Control)
}
