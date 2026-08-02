use librazer::device::Device;

use crate::{
    error::AppResult,
    razer::{
        enums::{BATTERY_LIMITS, BatteryLimit},
        protocol::{command, command_without_settings_update},
    },
    ui::{app::app, app_events::OsdEvent, theme::TOTAL_ANIM_TIME_MS},
};
use std::time::{Duration, Instant};

/// Battery handler.
///
/// Owns battery-care HID commands and the OSD cycle timeout.
pub struct BatteryHandler<'a> {
    device: &'a Device,
    battery_cycle_timeout: &'a mut Instant,
}

impl<'a> BatteryHandler<'a> {
    pub fn new(device: &'a Device, battery_cycle_timeout: &'a mut Instant) -> Self {
        Self {
            device,
            battery_cycle_timeout,
        }
    }

    pub fn set_battery_limit(&mut self, limit: BatteryLimit) {
        let _ = command(self.device, 0x0712, &[limit as u8], None);
        let _ = command(self.device, 0x070f, &[10], None);
    }

    /// Reads the battery-care setting retained by the device firmware.
    pub fn battery_limit(&self) -> AppResult<BatteryLimit> {
        // Settings snapshots call this query. Avoid turning each read into a
        // new settings-update event, which would cause the UI to request
        // another snapshot indefinitely.
        // The firmware replaces request argument 0 with the current
        // battery-care level in its first response argument.
        let response = command_without_settings_update(self.device, 0x0792, &[0], Some(&[0]))?;
        Ok(response
            .first()
            .copied()
            .map(BatteryLimit::from)
            .unwrap_or(BatteryLimit::Unknown))
    }

    pub fn cycle_battery_limit(&mut self) {
        let mut current_limit = self.battery_limit().unwrap_or(BatteryLimit::Unknown);
        if Instant::now() < *self.battery_cycle_timeout {
            let limit = next_battery_limit(current_limit);
            self.set_battery_limit(limit);
            current_limit = limit;
        }
        let index = BATTERY_LIMITS
            .iter()
            .position(|&limit| limit == current_limit)
            .unwrap_or_default();
        let length = BATTERY_LIMITS.len() - 1;
        app(OsdEvent::BatteryLimit(current_limit as u8, index as u8, length as u8).into());
        *self.battery_cycle_timeout =
            Instant::now() + Duration::from_millis(TOTAL_ANIM_TIME_MS as u64);
    }
}

fn next_battery_limit(current: BatteryLimit) -> BatteryLimit {
    let index = BATTERY_LIMITS
        .iter()
        .position(|&limit| limit == current)
        .unwrap_or_default();
    BATTERY_LIMITS[(index + 1) % BATTERY_LIMITS.len()]
}
