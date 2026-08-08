use crate::config::ThemeColor;
use crate::core::shared_state::IS_PLUGGED_IN;
use crate::razer::config::{DeviceState, PowerProfile, allowed_perf_modes};
use crate::razer::enums::{PerfMode, RGB_EFFECTS};
use crate::win::system::usbpcap::capture::CapturedCommand;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;

/// Top-level application configuration containing separate hardware states
/// for plugged-in and battery power modes.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct AppConfig {
    model_pid: String,
    pub model_name: String,
    power_state: DeviceState,
    battery_state: DeviceState,
    pub custom_mode_config: crate::razer::config::CustomModeConfig,
    #[serde(alias = "default_multimedia_keys")]
    pub primary_multimedia_keys: bool,
    pub advanced_experimental_features: bool,
    pub theme_color: ThemeColor,
    pub keyboard_width: u8,
    /// Command Lab saved commands: command name → captured commands.
    pub command_lab_commands: HashMap<String, Vec<CapturedCommand>>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model_pid: String::default(),
            model_name: String::default(),
            power_state: DeviceState::for_profile(PowerProfile::Ac),
            battery_state: DeviceState::for_profile(PowerProfile::Battery),
            custom_mode_config: Default::default(),
            primary_multimedia_keys: false,
            advanced_experimental_features: false,
            theme_color: ThemeColor::default(),
            keyboard_width: 0,
            command_lab_commands: HashMap::new(),
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
    pub fn get(&mut self) -> &mut DeviceState {
        self.profile_mut(Self::active_profile())
    }
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
    pub fn refresh_cycle_items(&mut self) {
        self.power_state.rgb_effect.items = RGB_EFFECTS.to_vec();
        refresh_perf_mode_items(&mut self.power_state, PowerProfile::Ac);
        self.battery_state.rgb_effect.items = RGB_EFFECTS.to_vec();
        refresh_perf_mode_items(&mut self.battery_state, PowerProfile::Battery);
        self.custom_mode_config.cpu_level = self.custom_mode_config.cpu_level.min(3);
        self.custom_mode_config.gpu_level = self.custom_mode_config.gpu_level.min(3);
    }
    pub fn set_device_model(&mut self, pid: String, name: String) {
        self.model_pid = pid;
        self.model_name = name;
    }
}

fn refresh_perf_mode_items(state: &mut DeviceState, profile: PowerProfile) {
    let current_mode = state.perf_mode.value();
    state.perf_mode.items = allowed_perf_modes(profile);
    if state.perf_mode.set(&current_mode).is_err() {
        let _ = state.perf_mode.set(&PerfMode::Balanced);
    }
}
