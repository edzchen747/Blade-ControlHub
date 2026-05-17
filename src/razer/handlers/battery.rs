use librazer::device::Device;

use crate::{
    config::persist_config,
    razer::{config::AppConfig, enums::BatteryLimit, protocol::command},
    ui::{app::app, app_events::OsdEvent},
    utils::persist::PersistBuffer,
};
use std::time::{Duration, Instant};

/// Battery handler.
///
/// Accepts references to the device, app config, persist buffer, and battery cycle timeout.
/// All logic is copied exactly from Executer.
pub struct BatteryHandler<'a> {
    device: &'a Device,
    app_config: &'a mut AppConfig,
    persist_buffer: &'a PersistBuffer,
    battery_cycle_timeout: &'a mut Instant,
}

impl<'a> BatteryHandler<'a> {
    pub fn new(
        device: &'a Device,
        app_config: &'a mut AppConfig,
        persist_buffer: &'a PersistBuffer,
        battery_cycle_timeout: &'a mut Instant,
    ) -> Self {
        Self {
            device,
            app_config,
            persist_buffer,
            battery_cycle_timeout,
        }
    }

    pub fn set_battery_limit(&mut self, limit: BatteryLimit) {
        let _ = command(self.device, 0x0712, &[limit as u8], None);
        let _ = command(self.device, 0x070f, &[10], None);
        self.persist_config();
    }

    pub fn cycle_battery_limit(&mut self) {
        if Instant::now() < *self.battery_cycle_timeout {
            let limit = self.app_config.battery_limit.next();
            self.set_battery_limit(limit);
        }
        let current_limit = self.app_config.battery_limit.value();
        let index = self.app_config.battery_limit.index;
        let length = self.app_config.battery_limit.items.len() - 1;
        app().send(OsdEvent::BatteryLimit(current_limit as u8, index as u8, length as u8).into());
        *self.battery_cycle_timeout = Instant::now() + Duration::from_millis(1500);
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn persist_config(&mut self) {
        persist_config(self.app_config, self.persist_buffer);
    }
}
