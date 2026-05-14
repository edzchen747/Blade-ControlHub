use serde::{Deserialize, Serialize};
use strum_macros::Display;

#[derive(Clone, Copy, Display, Debug, PartialEq, Serialize, Deserialize)]
pub enum PerfMode {
    Silent = 5,
    Quiet = 6,
    Balanced = 0,
    Performance = 2,
    Turbo = 1,
    Custom = 4,
    Unknown = 255,
}

impl From<u8> for PerfMode {
    fn from(value: u8) -> Self {
        match value {
            5 => Self::Silent,
            6 => Self::Quiet,
            0 => Self::Balanced,
            2 => Self::Performance,
            1 => Self::Turbo,
            4 => Self::Custom,
            _ => {
                println!("Unknown Performance Mode: {}", value);
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
    Starlight = 25,
    Reactive = 19,
    Unknown = 255,
}

impl From<u8> for RGBEffect {
    fn from(value: u8) -> Self {
        match value {
            4 => Self::Cycle,
            1 => Self::Wave,
            3 => Self::Breathe,
            5 => Self::Ambient,
            7 => Self::Starlight,
            25 => Self::Starlight,
            19 => Self::Reactive,
            _ => {
                println!("Unknown RGB Effect: {}", value);
                Self::Unknown
            }
        }
    }
}

#[derive(Clone, Copy, Display, Debug, PartialEq, Serialize, Deserialize)]
pub enum BatteryLimit {
    Off = 60,
    #[strum(serialize = "Limit: 50%")]
    Limit50 = 178,
    #[strum(serialize = "Limit: 55%")]
    Limit55 = 183,
    #[strum(serialize = "Limit: 60%")]
    Limit60 = 188,
    #[strum(serialize = "Limit: 65%")]
    Limit65 = 193,
    #[strum(serialize = "Limit: 70%")]
    Limit70 = 198,
    #[strum(serialize = "Limit: 75%")]
    Limit75 = 203,
    #[strum(serialize = "Limit: 80%")]
    Limit80 = 208,
    Unknown = 255,
}

impl From<u8> for BatteryLimit {
    fn from(value: u8) -> Self {
        match value {
            60 => Self::Off,
            178 => Self::Limit50,
            183 => Self::Limit55,
            188 => Self::Limit60,
            193 => Self::Limit65,
            198 => Self::Limit70,
            203 => Self::Limit75,
            208 => Self::Limit80,
            _ => {
                println!("Unknown Battery Health Optimizer Value: {}", value);
                Self::Unknown
            }
        }
    }
}

#[derive(Clone, Copy, Display, Debug, PartialEq, Serialize, Deserialize)]
pub enum LidLogoMode {
    On = 2,
    Breathing = 1,
    Off = 0,
}

pub const RGB_EFFECTS: [RGBEffect; 6] = [
    RGBEffect::Cycle,
    RGBEffect::Wave,
    RGBEffect::Breathe,
    RGBEffect::Ambient,
    RGBEffect::Starlight,
    RGBEffect::Reactive,
];

pub const PERF_MODES: [PerfMode; 6] = [
    PerfMode::Silent,
    PerfMode::Quiet,
    PerfMode::Balanced,
    PerfMode::Performance,
    PerfMode::Turbo,
    PerfMode::Custom,
];

pub const BATTERY_LIMITS: [BatteryLimit; 8] = [
    BatteryLimit::Off,
    BatteryLimit::Limit50,
    BatteryLimit::Limit55,
    BatteryLimit::Limit60,
    BatteryLimit::Limit65,
    BatteryLimit::Limit70,
    BatteryLimit::Limit75,
    BatteryLimit::Limit80,
];
