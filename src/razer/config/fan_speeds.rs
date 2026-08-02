use crate::razer::config::FanSpeedLimits;
use crate::razer::enums::PerfMode;
use serde::{Deserialize, Serialize};

/// Saved manual fan speeds for each performance mode. A value of zero leaves
/// that mode under the laptop firmware's automatic fan control.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct FanSpeeds {
    pub battery_saver: u8,
    pub silent: u8,
    pub quiet: u8,
    pub balanced: u8,
    pub performance: u8,
    pub turbo: u8,
    pub custom: u8,
}

impl FanSpeeds {
    pub fn get(&self, perf_mode: PerfMode) -> u8 {
        match perf_mode {
            PerfMode::BatterySaver => self.battery_saver,
            PerfMode::Silent => self.silent,
            PerfMode::Quiet => self.quiet,
            PerfMode::Balanced => self.balanced,
            PerfMode::Performance => self.performance,
            PerfMode::Turbo => self.turbo,
            PerfMode::Custom => self.custom,
            PerfMode::Unsupported | PerfMode::Unknown => 0,
        }
    }
    pub fn set(&mut self, perf_mode: PerfMode, speed: u8) {
        match perf_mode {
            PerfMode::BatterySaver => self.battery_saver = speed,
            PerfMode::Silent => self.silent = speed,
            PerfMode::Quiet => self.quiet = speed,
            PerfMode::Balanced => self.balanced = speed,
            PerfMode::Performance => self.performance = speed,
            PerfMode::Turbo => self.turbo = speed,
            PerfMode::Custom => self.custom = speed,
            PerfMode::Unsupported | PerfMode::Unknown => {}
        }
    }
    pub fn clamp_to_limits(&mut self, limits: FanSpeedLimits) {
        for speed in [
            &mut self.battery_saver,
            &mut self.silent,
            &mut self.quiet,
            &mut self.balanced,
            &mut self.performance,
            &mut self.turbo,
            &mut self.custom,
        ] {
            if *speed != 0 {
                *speed = (*speed).clamp(limits.min, limits.max);
            }
        }
    }
}
