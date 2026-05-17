use crate::razer::enums::LidLogoMode;

/// Trait abstracting hardware device control operations.
/// Implemented by DeviceHandle. Enables future unit testing via mock impls.
pub trait DeviceController {
    fn initialize(&self, notify_startup: bool);
    fn sleep(&self);
    fn shutdown(&self) -> bool;
    fn get_pid(&self) -> u16;
    fn cycle_perf_mode(&self);
    fn cycle_rgb_mode(&self);
    fn cycle_refresh_rate(&self);
    fn cycle_battery_limit(&self);
    fn toggle_vc(&self);
    fn keyboard_light_up(&self);
    fn keyboard_light_down(&self);
    fn adjust_screen_brightness(&self, change: i8);
    fn set_lid_logo(&self, mode: LidLogoMode);
    fn persist_config(&self);
}
