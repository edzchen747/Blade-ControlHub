pub mod hidapi;
pub mod key_hook;
pub mod key_map;
pub mod razer_key;
pub mod trackpad;
pub mod vkey;

use crate::error::AppResult;

pub fn start_keyboard_hooks(device_pid: u16) -> AppResult<()> {
    key_hook::KeyHook::start();
    hidapi::HidApiListener::new(device_pid).start()
}

pub fn stop_keyboard_hooks() {
    key_hook::KeyHook::stop();
    hidapi::HidApiListener::stop();
}

/// Drops input listeners opened before sleep and recreates them for the
/// freshly enumerated Razer PID after resume.
pub fn reinitialize_keyboard_hooks(device_pid: u16) -> AppResult<()> {
    stop_keyboard_hooks();
    start_keyboard_hooks(device_pid)
}

#[derive(PartialEq, Eq, Hash)]
pub enum KeyType {
    VKey(vkey::Key),
    RazerKey(razer_key::Key),
}

impl From<vkey::Key> for KeyType {
    fn from(s: vkey::Key) -> Self {
        KeyType::VKey(s)
    }
}

impl From<razer_key::Key> for KeyType {
    fn from(b: razer_key::Key) -> Self {
        KeyType::RazerKey(b)
    }
}
