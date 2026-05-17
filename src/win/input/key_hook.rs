use std::sync::atomic::Ordering;

use rdev::{Event, EventType, Key, grab};

use crate::win::input::{
    key_map::{ALT_PRESSED, KEY_MAP},
    scancode,
};

pub struct KeyHook {}

impl KeyHook {
    pub fn start() {
        std::thread::spawn(|| {
            println!("\n--- KeyHook grab thread started. ---");
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

        let key = scancode::Key::from(rdev_key);
        if key == scancode::Key::Unknown {
            return Some(event);
        }

        if let Some(event_action) = KEY_MAP.get(&key)
            && event_action.execute()
        {
            return None;
        }
    }
    if let EventType::KeyRelease(key) = event.event_type
        && key == Key::Alt
    {
        ALT_PRESSED.store(false, Ordering::SeqCst);
    }
    Some(event)
}
