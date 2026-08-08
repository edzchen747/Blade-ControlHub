use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use blade_controlhub::error::AppResult;
use blade_controlhub::hal::DeviceController;
use blade_controlhub::razer::enums::LidLogoMode;

#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
pub struct MockDeviceController {
    pub initialize_called: Arc<AtomicBool>,
    pub cycle_perf_called: Arc<AtomicBool>,
    pub cycle_rgb_called: Arc<AtomicBool>,
    pub persist_called: Arc<AtomicBool>,
    pub pid: Arc<AtomicU16>,
}

impl MockDeviceController {
    pub fn new() -> Self {
        Self::default()
    }
}

impl DeviceController for MockDeviceController {
    fn initialize(&self, _notify_startup: bool) {
        self.initialize_called.store(true, Ordering::SeqCst);
    }

    fn sleep(&self) -> AppResult<bool> {
        Ok(true)
    }

    fn shutdown(&self) -> AppResult<bool> {
        Ok(true)
    }

    fn get_pid(&self) -> AppResult<u16> {
        Ok(self.pid.load(Ordering::SeqCst))
    }

    fn cycle_perf_mode(&self) {
        self.cycle_perf_called.store(true, Ordering::SeqCst);
    }

    fn cycle_rgb_mode(&self) {
        self.cycle_rgb_called.store(true, Ordering::SeqCst);
    }

    fn cycle_refresh_rate(&self) {}
    fn cycle_battery_limit(&self) {}
    fn toggle_vc(&self) {}
    fn keyboard_light_up(&self) {}
    fn keyboard_light_down(&self) {}
    fn adjust_screen_brightness(&self, _change: i8) {}
    fn set_lid_logo(&self, _mode: LidLogoMode) {}

    fn persist_config(&self) {
        self.persist_called.store(true, Ordering::SeqCst);
    }
}
