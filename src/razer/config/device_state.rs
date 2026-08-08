use crate::razer::config::{CycleState, FanSpeeds, PowerProfile, allowed_perf_modes};
use crate::razer::enums::{PERF_MODES, PerfMode, RGB_EFFECTS, RGBEffect};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct DeviceState {
    pub key_lvl: u8,
    #[serde(default = "default_rgb_effect")]
    pub rgb_effect: CycleState<RGBEffect>,
    pub vc_lvl: u8,
    #[serde(default = "default_perf_mode")]
    pub perf_mode: CycleState<PerfMode>,
    #[serde(default)]
    pub fan_speeds: FanSpeeds,
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
            fan_speeds: FanSpeeds::default(),
            screen_lvl: 100,
            screen_refresh: 0,
        }
    }
}

impl DeviceState {
    pub(super) fn for_profile(profile: PowerProfile) -> Self {
        let mut perf_mode = CycleState::new(allowed_perf_modes(profile));
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
