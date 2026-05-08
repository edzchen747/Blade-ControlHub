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
    Unknown = 255,
}

impl From<u8> for RGBEffect {
    fn from(value: u8) -> Self {
        match value {
            4 => Self::Cycle,
            1 => Self::Wave,
            3 => Self::Breathe,
            5 => Self::Ambient,
            _ => {
                println!("Unknown RGB Effect: {}", value);
                Self::Unknown
            }
        }
    }
}

pub const RGB_EFFECTS: [RGBEffect; 4] = [
    RGBEffect::Cycle,
    RGBEffect::Wave,
    RGBEffect::Breathe,
    RGBEffect::Ambient,
];

pub const PERF_MODES: [PerfMode; 6] = [
    PerfMode::Silent,
    PerfMode::Quiet,
    PerfMode::Balanced,
    PerfMode::Performance,
    PerfMode::Turbo,
    PerfMode::Custom,
];
