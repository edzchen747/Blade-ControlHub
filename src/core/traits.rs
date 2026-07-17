use crate::error::AppResult;
use crate::razer::enums::LidLogoMode;

/// Trait abstracting hardware device control operations.
/// Implemented by both DeviceHandle (production) and MockDeviceController (tests).
#[expect(dead_code)]
pub trait DeviceController {
    fn initialize(&self, notify_startup: bool);
    fn sleep(&self) -> AppResult<bool>;
    fn shutdown(&self) -> AppResult<bool>;
    fn get_pid(&self) -> AppResult<u16>;
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
