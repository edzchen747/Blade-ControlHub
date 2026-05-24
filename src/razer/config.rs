use crate::error::{AppError, AppResult};
use crate::razer::enums::*;
use crate::win::input::key_map::SHIFT_PRESSED;
use crate::win::system::power::IS_PLUGGED_IN;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};

// ── CycleState ──────────────────────────────────────────────────────────────

/// A generic cyclic iterator over a fixed set of items.
/// Tracks a current index and advances through the collection in a loop.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CycleState<T> {
    pub index: usize,
    pub items: Vec<T>,
}

impl<T: Clone + PartialEq> CycleState<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { index: 0, items }
    }

    /// Advances to the next item and returns it.
    pub fn next(&mut self) -> T {
        debug_assert!(
            !self.items.is_empty(),
            "CycleState::next called on empty items"
        );
        if self.items.is_empty() {
            return self.items[0].clone(); // unreachable in practice; debug_assert fires in dev
        }
        let reverse = SHIFT_PRESSED.load(Ordering::SeqCst);
        let shift = if reverse { self.items.len() - 1 } else { 1 };
        self.index = (self.index + shift) % self.items.len();
        self.items[self.index].clone()
    }

    /// Returns the current item without advancing.
    pub fn value(&mut self) -> T {
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
    pub battery_limit: CycleState<BatteryLimit>,
    pub default_multimedia_keys: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            power_state: DeviceState::default(),
            battery_state: DeviceState::default(),
            battery_limit: CycleState {
                index: 0,
                items: BATTERY_LIMITS.to_vec(),
            },
            default_multimedia_keys: false,
        }
    }
}

impl AppConfig {
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
}
