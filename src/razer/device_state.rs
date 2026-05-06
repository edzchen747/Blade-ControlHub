use crate::win::brightness::SCREEN_TARGET_LVL;
use crate::win::persist::PersistBuffer;
use crate::win::power::IS_PLUGGED_IN;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use strum_macros::Display;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct AppConfig {
    power_state: DeviceState,
    battery_state: DeviceState,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl AppConfig {
    fn new() -> Self {
        Self::from(DeviceState::default(), DeviceState::default())
    }

    pub fn from(power_state: DeviceState, battery_state: DeviceState) -> Self {
        Self {
            power_state,
            battery_state,
        }
    }

    pub fn get(&mut self) -> &mut DeviceState {
        match IS_PLUGGED_IN.load(Ordering::SeqCst) {
            true => &mut self.power_state,
            false => &mut self.battery_state,
        }
    }

    pub fn read(&self) -> DeviceState {
        let state_config = match IS_PLUGGED_IN.load(Ordering::SeqCst) {
            true => &self.power_state,
            false => &self.battery_state,
        };
        state_config.clone()
    }
}

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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CycleState<T> {
    index: usize,
    pub items: Vec<T>,
}

impl<T: Clone + PartialEq> CycleState<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { index: 0, items }
    }

    pub fn next(&mut self) -> T
    where
        T: Clone,
    {
        if self.items.is_empty() {
            panic!("Cannot get next value from an empty collection");
        }
        self.index = (self.index + 1) % self.items.len();
        self.items[self.index].clone()
    }

    pub fn value(&mut self) -> T {
        self.items[self.index].clone()
    }

    pub fn set(&mut self, value: &T) {
        if let Some(pos) = self.items.iter().position(|x| x == value) {
            self.index = pos;
        } else {
            panic!("Internal State Error");
        }
    }
}

const RGB_EFFECTS: [RGBEffect; 4] = [
    RGBEffect::Cycle,
    RGBEffect::Wave,
    RGBEffect::Breathe,
    RGBEffect::Ambient,
];
const PERF_MODES: [PerfMode; 6] = [
    PerfMode::Silent,
    PerfMode::Quiet,
    PerfMode::Balanced,
    PerfMode::Performance,
    PerfMode::Turbo,
    PerfMode::Custom,
];
pub const CONFIG_PATH: &str = "config.json";

pub fn load_config() -> AppConfig {
    let Ok(contents) = std::fs::read_to_string(CONFIG_PATH) else {
        println!("Config not found, using defaults.");
        return AppConfig::default();
    };

    if contents.trim().is_empty() {
        return AppConfig::default();
    }

    let mut app_config = serde_json::from_str(&contents).unwrap_or_else(|e| {
        println!("Failed to parse config: {}. Using defaults.", e);
        AppConfig::default()
    });

    // Overide saved values incase new updates bring more options
    app_config.power_state.rgb_effect.items = RGB_EFFECTS.to_vec();
    app_config.power_state.perf_mode.items = PERF_MODES.to_vec();
    app_config.battery_state.rgb_effect.items = RGB_EFFECTS.to_vec();
    app_config.battery_state.perf_mode.items = PERF_MODES.to_vec();
    app_config
}

pub fn persist_config(app_config: &mut AppConfig, persist_buffer: &PersistBuffer) {
    app_config.get().screen_lvl = SCREEN_TARGET_LVL.load(Ordering::SeqCst);
    if let Ok(json) = serde_json::to_string_pretty(app_config) {
        let _ = persist_buffer.write(json);
    }
}

#[derive(Clone, Copy, Display, Debug, PartialEq, Serialize, Deserialize)]
pub enum PerfMode {
    Silent = 5,
    Quiet = 6, // Quiet? (This is not exposed in Syanpse)
    Balanced = 0,
    Performance = 2,
    Turbo = 1,
    Custom = 4,
    Unknown = 255,
}

impl From<u8> for PerfMode {
    fn from(perf_mode: u8) -> Self {
        match perf_mode {
            5 => Self::Silent,
            6 => Self::Quiet,
            0 => Self::Balanced,
            2 => Self::Performance,
            1 => Self::Turbo,
            4 => Self::Custom,
            _ => {
                println!("Unknown Performance Mode: {}", perf_mode);
                Self::Unknown
            }
        }
    }
}

#[derive(Clone, Copy, Display, Debug, PartialEq, Serialize, Deserialize)]
pub enum RGBEffect {
    Cycle = 4,
    Wave = 1,
    Breathe = 3,
    Ambient = 5,
    Unknown = 255,
}

impl From<u8> for RGBEffect {
    fn from(rgb_effect: u8) -> Self {
        match rgb_effect {
            4 => Self::Cycle,
            1 => Self::Wave,
            3 => Self::Breathe,
            5 => Self::Ambient,
            _ => {
                println!("Unknown RGB Effect: {}", rgb_effect);
                Self::Unknown
            }
        }
    }
}
