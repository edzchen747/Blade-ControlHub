use crate::config::ThemeColor;
use crate::core::shared_state::IS_PLUGGED_IN;
use crate::razer::config::{
    CustomToggle, DeviceState, HiddenDashboardControls, PowerProfile, allowed_perf_modes,
};
use crate::razer::enums::{PerfMode, RGB_EFFECTS};
use crate::win::input::binding::KeyBindings;
use crate::win::system::usbpcap::capture::CapturedCommand;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;

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
    pub start_with_admin: bool,
    pub start_with_windows: bool,
    pub command_lab_commands: HashMap<String, Vec<CapturedCommand>>,
    pub custom_toggles: Vec<CustomToggle>,
    pub hidden_dashboard_controls: HiddenDashboardControls,
    pub key_bindings: KeyBindings,
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
            start_with_admin: false,
            start_with_windows: false,
            command_lab_commands: HashMap::new(),
            custom_toggles: Vec::new(),
            hidden_dashboard_controls: HiddenDashboardControls::default(),
            key_bindings: KeyBindings::default(),
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
        // Controls saved before their sides were remembered per profile carry
        // one side for both; this is the load-time pass that adopts it.
        for toggle in &mut self.custom_toggles {
            toggle.adopt_legacy_side();
        }
        self.power_state.rgb_effect.items = RGB_EFFECTS.to_vec();
        refresh_perf_mode_items(&mut self.power_state, PowerProfile::Ac);
        self.battery_state.rgb_effect.items = RGB_EFFECTS.to_vec();
        refresh_perf_mode_items(&mut self.battery_state, PowerProfile::Battery);
        self.custom_mode_config.cpu_level = self.custom_mode_config.cpu_level.min(3);
        self.custom_mode_config.gpu_level = self.custom_mode_config.gpu_level.min(3);
    }
    /// The captures to replay when `profile` becomes the one running: the side
    /// each usable control was left on.
    ///
    /// A control hidden from the Dashboard is left out. Hiding it takes it out
    /// of both profiles rather than only out of the list — a control the user
    /// has put away must not keep flipping itself every time the charger moves.
    /// It still works where it is still offered: the Command Lab page's own
    /// switch, and any key bound to it.
    pub fn profile_toggle_captures(&self, profile: PowerProfile) -> Vec<String> {
        self.custom_toggles
            .iter()
            .filter(|toggle| {
                toggle.is_complete() && !self.hidden_dashboard_controls.hides_control(&toggle.name)
            })
            .map(|toggle| toggle.capture_for(toggle.enabled(profile)).to_owned())
            .collect()
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
