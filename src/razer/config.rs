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

    pub fn remove(&mut self, value: &T) -> bool {
        let Some(pos) = self.items.iter().position(|x| x == value) else {
            return false;
        };

        self.items.remove(pos);
        if self.items.is_empty() {
            self.index = 0;
        } else if pos < self.index {
            self.index -= 1;
        } else if self.index >= self.items.len() {
            self.index = 0;
        }

        true
    }

    pub fn remove_for_cycle_retry(&mut self, value: &T) -> bool {
        let Some(pos) = self.items.iter().position(|x| x == value) else {
            return false;
        };
        let reverse = SHIFT_PRESSED.load(Ordering::SeqCst);

        self.items.remove(pos);
        if self.items.is_empty() {
            self.index = 0;
            return true;
        }

        let next_candidate_pos = if reverse {
            pos.checked_sub(1).unwrap_or_else(|| self.items.len() - 1)
        } else if pos >= self.items.len() {
            0
        } else {
            pos
        };
        self.index = if reverse {
            (next_candidate_pos + 1) % self.items.len()
        } else {
            next_candidate_pos
                .checked_sub(1)
                .unwrap_or_else(|| self.items.len() - 1)
        };

        true
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

impl DeviceState {
    fn for_profile(profile: PowerProfile) -> Self {
        let mut perf_mode = CycleState::new(allowed_perf_modes(profile).to_vec());
        let default_mode = match profile {
            PowerProfile::Ac => PerfMode::Balanced,
            PowerProfile::Battery => PerfMode::Silent,
        };
        let _ = perf_mode.set(&default_mode);

        Self {
            perf_mode,
            ..Self::default()
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
            power_state: DeviceState::for_profile(PowerProfile::Ac),
            battery_state: DeviceState::for_profile(PowerProfile::Battery),
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

pub fn allowed_perf_modes(profile: PowerProfile) -> &'static [PerfMode] {
    match profile {
        PowerProfile::Ac => &[
            PerfMode::Silent,
            PerfMode::Quiet,
            PerfMode::Balanced,
            PerfMode::Performance,
            PerfMode::Turbo,
            PerfMode::Custom,
        ],
        PowerProfile::Battery => &[
            PerfMode::BatterySaver,
            PerfMode::Silent,
            PerfMode::Quiet,
            PerfMode::Balanced,
        ],
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
        refresh_perf_mode_items(&mut self.power_state, PowerProfile::Ac);
        self.battery_state.rgb_effect.items = RGB_EFFECTS.to_vec();
        refresh_perf_mode_items(&mut self.battery_state, PowerProfile::Battery);
    }

    pub fn set_device_model(&mut self, pid: String, name: String) {
        self.model_pid = pid;
        self.model_name = name;
    }
}

fn refresh_perf_mode_items(state: &mut DeviceState, profile: PowerProfile) {
    let current_mode = state.perf_mode.value();
    state.perf_mode.items = allowed_perf_modes(profile).to_vec();
    if state.perf_mode.set(&current_mode).is_err() {
        let _ = state.perf_mode.set(&PerfMode::Balanced);
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
    fn default_perf_mode_is_profile_specific() {
        let mut config = AppConfig::default();

        assert_eq!(
            config.profile_mut(PowerProfile::Ac).perf_mode.value(),
            PerfMode::Balanced
        );
        assert_eq!(
            config.profile_mut(PowerProfile::Battery).perf_mode.value(),
            PerfMode::Silent
        );
    }

    #[test]
    fn refresh_cycle_items_uses_profile_specific_perf_modes() {
        let config = AppConfig::default();

        assert_eq!(
            config.profile(PowerProfile::Ac).perf_mode.items,
            allowed_perf_modes(PowerProfile::Ac).to_vec()
        );
        assert_eq!(
            config.profile(PowerProfile::Battery).perf_mode.items,
            allowed_perf_modes(PowerProfile::Battery).to_vec()
        );
    }

    #[test]
    fn refresh_cycle_items_preserves_allowed_perf_mode_by_value() {
        let mut config = AppConfig::default();
        config
            .profile_mut(PowerProfile::Ac)
            .perf_mode
            .set(&PerfMode::Turbo)
            .expect("turbo is allowed on AC");

        config.refresh_cycle_items();

        assert_eq!(
            config.profile_mut(PowerProfile::Ac).perf_mode.value(),
            PerfMode::Turbo
        );
    }

    #[test]
    fn refresh_cycle_items_falls_back_to_balanced_when_mode_is_disallowed() {
        let mut config = AppConfig::default();
        config.power_state.perf_mode.items = PERF_MODES.to_vec();
        config
            .power_state
            .perf_mode
            .set(&PerfMode::BatterySaver)
            .expect("test list contains battery saver");

        config.refresh_cycle_items();

        assert_eq!(
            config.profile_mut(PowerProfile::Ac).perf_mode.value(),
            PerfMode::Balanced
        );
        assert!(
            !config
                .profile(PowerProfile::Ac)
                .perf_mode
                .items
                .contains(&PerfMode::BatterySaver)
        );
    }
}
