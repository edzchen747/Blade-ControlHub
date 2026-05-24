use rdev::{Event, EventType, Key, grab};
use std::sync::atomic::Ordering;
use tracing::{error, info};

use crate::win::input::{
    key_map::{ALT_PRESSED, KEY_MAP, SHIFT_PRESSED},
    vkey,
};

pub struct KeyHook {}

impl KeyHook {
    pub fn start() {
        std::thread::spawn(|| {
            info!("Keyboard grab thread started");
            if let Err(e) = grab(key_event_handler) {
                error!(error = ?e, "Keyboard grab failed");
            }
        });
    }
}

fn key_event_handler(event: Event) -> Option<Event> {
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

        if let Some(event_action) = KEY_MAP.get(&key.into())
            && event_action.execute()
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
