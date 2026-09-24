use librazer::device::Device;

use crate::{
    core::shared_state::SHIFT_PRESSED,
    error::AppResult,
    razer::{
        enums::{BATTERY_LIMITS, BatteryLimit},
        protocol::{command, command_without_settings_update},
    },
    ui::{app::app, app_events::OsdEvent, theme::TOTAL_ANIM_TIME_MS},
};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

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

    /// Advances the limit and returns the value now on the device, so the
    /// caller can keep the settings snapshot's cache in step.
    pub fn cycle_battery_limit(&mut self) -> BatteryLimit {
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
        current_limit
    }
}

/// The next limit in the cycle, or the previous one while Shift is held.
///
/// Battery limit is the one cycling control that does not go through
/// `CycleState`, because its value is read back from the firmware rather than
/// tracked in the config. It still has to honour Shift the same way.
fn next_battery_limit(current: BatteryLimit) -> BatteryLimit {
    step_battery_limit(current, SHIFT_PRESSED.load(Ordering::SeqCst))
}

fn step_battery_limit(current: BatteryLimit, reverse: bool) -> BatteryLimit {
    let length = BATTERY_LIMITS.len();
    let index = BATTERY_LIMITS
        .iter()
        .position(|&limit| limit == current)
        .unwrap_or_default();
    let next = if reverse {
        index.checked_sub(1).unwrap_or(length - 1)
    } else {
        (index + 1) % length
    };
    BATTERY_LIMITS[next]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycling_forward_walks_the_list_and_wraps() {
        assert_eq!(
            step_battery_limit(BatteryLimit::Off, false),
            BatteryLimit::Limit50
        );
        assert_eq!(
            step_battery_limit(BatteryLimit::Limit80, false),
            BatteryLimit::Off
        );
    }

    #[test]
    fn holding_shift_walks_the_list_backwards_and_wraps() {
        assert_eq!(
            step_battery_limit(BatteryLimit::Limit50, true),
            BatteryLimit::Off
        );
        assert_eq!(
            step_battery_limit(BatteryLimit::Off, true),
            BatteryLimit::Limit80
        );
    }

    #[test]
    fn an_unknown_limit_starts_the_cycle_from_the_first_entry() {
        assert_eq!(
            step_battery_limit(BatteryLimit::Unknown, false),
            BatteryLimit::Limit50
        );
        assert_eq!(
            step_battery_limit(BatteryLimit::Unknown, true),
            BatteryLimit::Limit80
        );
    }
}
