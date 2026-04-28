use crate::razer::executer::Executer;
use crate::win::persist::PersistBuffer;

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AppConfig {
    pub power_state: DeviceState,
    pub battery_state: DeviceState,
}

impl AppConfig {
    fn from(power_state: DeviceState, battery_state: DeviceState) -> Self {
        Self {
            power_state,
            battery_state,
        }
    }
    fn new() -> Self {
        Self::from(DeviceState::new(), DeviceState::new())
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DeviceState {
    pub last_key_lvl: u8,
    pub rgb_effect: CycleState<u8>,
    pub last_vc_lvl: u8,
    pub perf_mode: CycleState<u8>,
}

impl DeviceState {
    fn new() -> Self {
        Self {
            last_key_lvl: 255,
            rgb_effect: CycleState::new(RGB_EFFECTS.to_vec()),
            last_vc_lvl: 255,
            perf_mode: CycleState::new(PERF_MODES.to_vec()),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CycleState<T> {
    index: usize,
    items: Vec<T>,
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

const RGB_EFFECTS: [u8; 3] = [4, 1, 3];
const PERF_MODES: [u8; 5] = [5, 6, 2, 1, 4];
pub const CONFIG_PATH: &str = "config.json";

pub fn load_config() -> AppConfig {
    let file_result = File::open("config.json");
    let mut file = match file_result {
        Ok(file) => file,
        Err(error) => match error.kind() {
            std::io::ErrorKind::NotFound => {
                println!("Creating config.json file");
                File::create(CONFIG_PATH).expect("Fatal error: unable to create config file")
            }
            _ => panic!("Problem opening the file: {:?}", error),
        },
    };

    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap_or_default();
    serde_json::from_str(&contents).unwrap_or_else(|_| AppConfig::new())
}

pub fn save_config(app_config: &AppConfig, persist_buffer: &PersistBuffer) {
    if let Ok(json) = serde_json::to_string_pretty(app_config) {
        let _ = persist_buffer.write(json);
    }
}

pub enum ChargeState {
    Battery,
    Power,
}
