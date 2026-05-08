use crate::razer::enums::*;
use crate::utils::persist::PersistBuffer;
use crate::win::display::brightness::SCREEN_TARGET_LVL;
use crate::win::system::power::IS_PLUGGED_IN;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};

// ── Config Path ─────────────────────────────────────────────────────────────

pub const CONFIG_PATH: &str = "config.json";

// ── CycleState ──────────────────────────────────────────────────────────────

/// A generic cyclic iterator over a fixed set of items.
/// Tracks a current index and advances through the collection in a loop.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CycleState<T> {
    index: usize,
    pub items: Vec<T>,
}

impl<T: Clone + PartialEq> CycleState<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { index: 0, items }
    }

    /// Advances to the next item and returns it.
    pub fn next(&mut self) -> T {
        if self.items.is_empty() {
            panic!("Cannot get next value from an empty collection");
        }
        self.index = (self.index + 1) % self.items.len();
        self.items[self.index].clone()
    }

    /// Returns the current item without advancing.
    pub fn value(&mut self) -> T {
        self.items[self.index].clone()
    }

    /// Sets the current index to the position of the given value.
    pub fn set(&mut self, value: &T) {
        if let Some(pos) = self.items.iter().position(|x| x == value) {
            self.index = pos;
        } else {
            panic!("Internal State Error");
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
    CycleState::new(PERF_MODES.to_vec())
}

// ── AppConfig ───────────────────────────────────────────────────────────────

/// Top-level application configuration containing separate hardware states
/// for plugged-in and battery power modes.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct AppConfig {
    power_state: DeviceState,
    battery_state: DeviceState,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            power_state: DeviceState::default(),
            battery_state: DeviceState::default(),
        }
    }
}

impl AppConfig {
    pub fn from(power_state: DeviceState, battery_state: DeviceState) -> Self {
        Self {
            power_state,
            battery_state,
        }
    }

    /// Returns a mutable reference to the active power state.
    pub fn get(&mut self) -> &mut DeviceState {
        if IS_PLUGGED_IN.load(Ordering::SeqCst) {
            &mut self.power_state
        } else {
            &mut self.battery_state
        }
    }

    /// Returns a clone of the active power state.
    pub fn read(&self) -> DeviceState {
        if IS_PLUGGED_IN.load(Ordering::SeqCst) {
            self.power_state.clone()
        } else {
            self.battery_state.clone()
        }
    }
}

// ── Config Persistence ──────────────────────────────────────────────────────

/// Loads the application config from disk, falling back to defaults on error.
pub fn load_config() -> AppConfig {
    let Ok(contents) = std::fs::read_to_string(CONFIG_PATH) else {
        println!("Config not found, using defaults.");
        return AppConfig::default();
    };

    if contents.trim().is_empty() {
        return AppConfig::default();
    }

    let mut app_config: AppConfig = serde_json::from_str(&contents).unwrap_or_else(|e| {
        println!("Failed to parse config: {}. Using defaults.", e);
        AppConfig::default()
    });

    // Override saved cycle items in case new updates bring more options
    app_config.power_state.rgb_effect.items = RGB_EFFECTS.to_vec();
    app_config.power_state.perf_mode.items = PERF_MODES.to_vec();
    app_config.battery_state.rgb_effect.items = RGB_EFFECTS.to_vec();
    app_config.battery_state.perf_mode.items = PERF_MODES.to_vec();
    app_config
}

/// Persists the current application config to disk via the provided buffer.
pub fn persist_config(app_config: &mut AppConfig, persist_buffer: &PersistBuffer) {
    app_config.get().screen_lvl = SCREEN_TARGET_LVL.load(Ordering::SeqCst);
    if let Ok(json) = serde_json::to_string_pretty(app_config) {
        let _ = persist_buffer.write(json);
    }
}
