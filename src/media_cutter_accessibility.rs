//! Cutter announcements use the same direct VoiceOver output as the player.
#[cfg(target_os = "macos")]
pub(super) fn announce_voiceover_message(message: &str) {
    use std::sync::{Arc, Condvar, Mutex, OnceLock};
    type Pending = Arc<(Mutex<Option<String>>, Condvar)>;
    static PENDING: OnceLock<Pending> = OnceLock::new();
    let message = message.trim();
    if message.is_empty() {
        return;
    }
    let queue = PENDING.get_or_init(|| {
        let queue: Pending = Arc::new((Mutex::new(None), Condvar::new()));
        let worker = Arc::clone(&queue);
        std::thread::spawn(move || {
            loop {
                let (lock, wake) = &*worker;
                let message = {
                    let mut pending = lock.lock().unwrap_or_else(|e| e.into_inner());
                    while pending.is_none() {
                        pending = wake.wait(pending).unwrap_or_else(|e| e.into_inner());
                    }
                    pending.take().unwrap()
                };
                crate::deliver_voiceover_message(&message);
            }
        });
        queue
    });
    let (lock, wake) = &**queue;
    *lock.lock().unwrap_or_else(|e| e.into_inner()) = Some(message.to_owned());
    wake.notify_one();
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
