pub mod blocker;
pub mod hidapi;
pub mod key;
pub mod key_map;
pub mod raw_input;
pub mod trackpad;

pub fn start_keyboard_hooks(device_pid: u16) {
    raw_input::RawInputListener::start();
    hidapi::HidApiListener::new(device_pid).start();
    blocker::KeyBlocker::start();
}
