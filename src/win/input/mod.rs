pub mod hidapi;
pub mod key_hook;
pub mod key_map;
pub mod scancode;
pub mod trackpad;

use crate::error::AppResult;

pub fn start_keyboard_hooks(device_pid: u16) -> AppResult<()> {
    key_hook::KeyHook::start();
    hidapi::HidApiListener::new(device_pid).start()
}
