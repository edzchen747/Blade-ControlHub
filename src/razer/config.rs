use crate::config::ThemeColor;
use crate::core::shared_state::{IS_PLUGGED_IN, SHIFT_PRESSED};
use crate::error::{AppError, AppResult};
use crate::razer::enums::*;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerProfile {
    #[default]
    Ac,
    Battery,
}

// ── CycleState ──────────────────────────────────────────────────────────────

/// A generic cyclic iterator over a fixed set of items.
/// Tracks a current index and advances through the collection in a loop.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CycleState<T> {
    pub index: usize,
    pub items: Vec<T>,
}

impl<T: Clone + PartialEq + Default> CycleState<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { index: 0, items }
    }

    /// Advances to the next item and returns it.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> T {
        if self.items.is_empty() {
            self.index = 0;
            return T::default();
        }

        let reverse = SHIFT_PRESSED.load(Ordering::SeqCst);
        let current_index = self.normalized_index();
        self.index = if reverse {
            current_index
                .checked_sub(1)
                .unwrap_or_else(|| self.items.len() - 1)
        } else {
            (current_index + 1) % self.items.len()
        };
        self.items[self.index].clone()
    }

    /// Returns the current item without advancing.
    pub fn value(&mut self) -> T {
        if self.items.is_empty() {
            self.index = 0;
            return T::default();
        }

        self.index = self.normalized_index();
        self.items[self.index].clone()
    }

    /// Sets the current index to the position of the given value.
    pub fn set(&mut self, value: &T) -> AppResult<()> {
        match self.items.iter().position(|x| x == value) {
            Some(pos) => {
                self.index = pos;
                Ok(())
            }
            None => Err(AppError::Internal(
                "CycleState::set: value not found in items list".to_string(),
            )),
        }
    }

    fn normalized_index(&self) -> usize {
        if self.index < self.items.len() {
            self.index
        } else {
            0
        }
    }
}

// ── DeviceState ─────────────────────────────────────────────────────────────

/// Hardware settings for a single power state (plugged in or on battery).
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct DeviceState {
    pub key_lvl: u8,
    #[serde(default = "default_rgb_effect")]
    pub rgb_effect: CycleState<RGBEffect>,
    pub vc_lvl: u8,
    #[serde(default = "default_perf_mode")]
    pub perf_mode: CycleState<PerfMode>,
    pub screen_lvl: u8,
    pub screen_refresh: u32,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            key_lvl: 255,
            rgb_effect: default_rgb_effect(),
            vc_lvl: 255,
            perf_mode: default_perf_mode(),
            screen_lvl: 100,
            screen_refresh: 0,
        }
    }
}

fn default_rgb_effect() -> CycleState<RGBEffect> {
    CycleState::new(RGB_EFFECTS.to_vec())
}

fn default_perf_mode() -> CycleState<PerfMode> {
    let mut perf_mode = CycleState::new(PERF_MODES.to_vec());
    let _ = perf_mode.set(&PerfMode::Balanced);
    perf_mode
}

// ── AppConfig ───────────────────────────────────────────────────────────────

/// Top-level application configuration containing separate hardware states
/// for plugged-in and battery power modes.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct AppConfig {
    model_pid: String,
    pub model_name: String,
    power_state: DeviceState,
    battery_state: DeviceState,
    pub battery_limit: CycleState<BatteryLimit>,
    pub default_multimedia_keys: bool,
    pub theme_color: ThemeColor,
    pub keyboard_width: u8,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model_pid: String::default(),
            model_name: String::default(),
            power_state: DeviceState::default(),
            battery_state: DeviceState::default(),
            battery_limit: CycleState {
                index: 0,
                items: BATTERY_LIMITS.to_vec(),
            },
            default_multimedia_keys: false,
            theme_color: ThemeColor::default(),
            keyboard_width: 0,
        }
    }
}

impl AppConfig {
    pub fn active_profile() -> PowerProfile {
        if IS_PLUGGED_IN.load(Ordering::SeqCst) {
            PowerProfile::Ac
        } else {
            PowerProfile::Battery
        }
    }

    /// Returns a mutable reference to the active power state.
    pub fn get(&mut self) -> &mut DeviceState {
        self.profile_mut(Self::active_profile())
    }

    /// Returns a clone of the active power state.
    pub fn read(&self) -> DeviceState {
        self.profile(Self::active_profile())
    }

    pub fn profile(&self, profile: PowerProfile) -> DeviceState {
        match profile {
            PowerProfile::Ac => self.power_state.clone(),
            PowerProfile::Battery => self.battery_state.clone(),
        }
    }

    pub fn profile_mut(&mut self, profile: PowerProfile) -> &mut DeviceState {
        match profile {
            PowerProfile::Ac => &mut self.power_state,
            PowerProfile::Battery => &mut self.battery_state,
        }
    }

    /// Refreshes the cycle item lists for both power states.
    ///
    /// This ensures that if new options were added in a newer version of the app,
    /// they are available even when loading an older config file.
    pub fn refresh_cycle_items(&mut self) {
        self.power_state.rgb_effect.items = RGB_EFFECTS.to_vec();
        self.power_state.perf_mode.items = PERF_MODES.to_vec();
        self.battery_state.rgb_effect.items = RGB_EFFECTS.to_vec();
        self.battery_state.perf_mode.items = PERF_MODES.to_vec();
    }

    pub fn set_device_model(&mut self, pid: String, name: String) {
        self.model_pid = pid;
        self.model_name = name;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_device_model_updates_pid_and_name() {
        let mut config = AppConfig::default();

        config.set_device_model("0x02c7".to_string(), "Razer Blade Test".to_string());

        let serialized = serde_json::to_value(&config).expect("config must serialize");
        assert_eq!(serialized["model_pid"], "0x02c7");
        assert_eq!(config.model_name, "Razer Blade Test");
    }

    #[test]
    fn app_config_defaults_to_gold_theme_color() {
        let config = AppConfig::default();

        assert_eq!(config.theme_color, ThemeColor::default());
    }

    #[test]
    fn profile_mut_updates_only_selected_profile() {
        let mut config = AppConfig::default();

        config.profile_mut(PowerProfile::Battery).key_lvl = 51;

        assert_eq!(config.profile(PowerProfile::Battery).key_lvl, 51);
        assert_eq!(config.profile(PowerProfile::Ac).key_lvl, 255);
    }

    #[test]
    fn default_perf_mode_remains_balanced() {
        let mut config = AppConfig::default();

        assert_eq!(
            config.profile_mut(PowerProfile::Ac).perf_mode.value(),
            PerfMode::Balanced
        );
        assert_eq!(
            config.profile_mut(PowerProfile::Battery).perf_mode.value(),
            PerfMode::Balanced
        );
    }
}
