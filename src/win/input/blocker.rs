use std::sync::atomic::Ordering;

use rdev::{Event, EventType, Key, grab};

use crate::win::input::{
    key,
    key_map::{ALT_PRESSED, KEY_MAP},
};

pub struct KeyBlocker {}

impl KeyBlocker {
    pub fn start() {
        std::thread::spawn(|| {
            println!("\n--- KeyBlocker grab thread started. ---");
            if let Err(e) = grab(key_event_handler) {
                eprintln!("Keyboard grab failed: {:?}", e);
            }
        });
    }
}

fn key_event_handler(event: Event) -> Option<Event> {
    if let EventType::KeyPress(rdev_key) = event.event_type {
        if rdev_key == Key::Alt {
            ALT_PRESSED.store(true, Ordering::SeqCst);
        }

        let key = key::Key::from(rdev_key);
        if key != key::Key::Unknown(0) {
            if let Some(event_action) = KEY_MAP.get(&key) {
                match event_action.execute() {
                    true => return None,
                    false => (),
                }
            }
        }
    } else if let EventType::KeyRelease(key) = event.event_type {
        if key == Key::Alt {
            ALT_PRESSED.store(false, Ordering::SeqCst);
        }
    }
    Some(event)
}
