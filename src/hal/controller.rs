use crate::error::AppResult;
use crate::razer::enums::LidLogoMode;

/// Mockable hardware control boundary used by production handles and tests.
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
