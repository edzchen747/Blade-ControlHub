pub mod hidapi;
pub mod key_hook;
pub mod key_map;
pub mod scancode;
pub mod trackpad;

pub fn start_keyboard_hooks(device_pid: u16) {
    hidapi::HidApiListener::new(device_pid).start();
    key_hook::KeyHook::start();
}
