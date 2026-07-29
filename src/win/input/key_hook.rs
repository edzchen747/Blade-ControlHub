use rdev::{Event, EventType, Key, grab};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{error, info};

use crate::core::shared_state::{ALT_PRESSED, SHIFT_PRESSED};
use crate::win::input::{key_map::KEY_MAP, vkey};

pub struct KeyHook {}

static KEY_HOOK_RUNNING: AtomicBool = AtomicBool::new(false);

impl KeyHook {
    pub fn start() {
        if KEY_HOOK_RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }

        if let Err(error) = std::thread::Builder::new()
            .name("blade-keyboard-grab".to_string())
            .spawn(|| {
                info!("Keyboard grab thread started");
                if let Err(e) = grab(key_event_handler) {
                    error!(error = ?e, "Keyboard grab failed");
                }
                KEY_HOOK_RUNNING.store(false, Ordering::SeqCst);
            })
        {
            KEY_HOOK_RUNNING.store(false, Ordering::SeqCst);
            error!(%error, "Failed to start keyboard grab thread");
        }
    }

    pub fn stop() {
        KEY_HOOK_RUNNING.store(false, Ordering::SeqCst);
        ALT_PRESSED.store(false, Ordering::SeqCst);
        SHIFT_PRESSED.store(false, Ordering::SeqCst);
    }
}

fn key_event_handler(event: Event) -> Option<Event> {
    if !KEY_HOOK_RUNNING.load(Ordering::SeqCst) {
        return Some(event);
    }

    if let EventType::KeyPress(rdev_key) = event.event_type {
        if rdev_key == Key::Alt {
            ALT_PRESSED.store(true, Ordering::SeqCst);
        }

        if rdev_key == Key::ShiftLeft || rdev_key == Key::ShiftRight {
            SHIFT_PRESSED.store(true, Ordering::SeqCst);
        }

        let key = vkey::Key::from(rdev_key);
        if key == vkey::Key::Unknown {
            return Some(event);
        }

        if KEY_MAP
            .get(&key.into())
            .is_some_and(|event_action| event_action.execute())
        {
            return None;
        }
    }
    if let EventType::KeyRelease(key) = event.event_type {
        if key == Key::Alt {
            ALT_PRESSED.store(false, Ordering::SeqCst);
        }

        if key == Key::ShiftLeft || key == Key::ShiftRight {
            SHIFT_PRESSED.store(false, Ordering::SeqCst);
        }
    }
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn key_event(key: Key) -> Event {
        Event {
            event_type: EventType::KeyPress(key),
            name: None,
            time: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn stop_clears_modifier_state() {
        ALT_PRESSED.store(true, Ordering::SeqCst);
        SHIFT_PRESSED.store(true, Ordering::SeqCst);
        KEY_HOOK_RUNNING.store(true, Ordering::SeqCst);

        KeyHook::stop();

        assert!(!KEY_HOOK_RUNNING.load(Ordering::SeqCst));
        assert!(!ALT_PRESSED.load(Ordering::SeqCst));
        assert!(!SHIFT_PRESSED.load(Ordering::SeqCst));
    }

    #[test]
    fn stopped_hook_passes_events_through() {
        KEY_HOOK_RUNNING.store(false, Ordering::SeqCst);

        assert!(key_event_handler(key_event(Key::Alt)).is_some());
        assert!(!ALT_PRESSED.load(Ordering::SeqCst));
    }
}
