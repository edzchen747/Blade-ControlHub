use serde::{Deserialize, Serialize};

use crate::config::ThemeColor;
use crate::razer::{
    config::PowerProfile,
    enums::{BatteryLimit, PerfMode, RGBEffect},
};
use crate::runtime::settings_state::SettingsState;

pub const PIPE_NAME: &str = r"\\.\pipe\BladeControlHub";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcRequest {
    GetSettingsState,
    SetDefaultMultimediaKeys {
        enabled: bool,
    },
    SetPerfMode {
        profile: PowerProfile,
        mode: PerfMode,
    },
    SetRefreshRate {
        profile: PowerProfile,
        hz: u32,
    },
    SetKeyboardBrightness {
        profile: PowerProfile,
        level: u8,
    },
    SetRgbEffect {
        profile: PowerProfile,
        effect: RGBEffect,
    },
    SetUnderGlow {
        profile: PowerProfile,
        enabled: bool,
    },
    SetBatteryLimit {
        limit: BatteryLimit,
    },
    SetThemeColor {
        color: ThemeColor,
    },
    BeginRazerKeyCapture,
    CancelRazerKeyCapture,
    PollCapturedRazerKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcResponse {
    SettingsState(SettingsState),
    CapturedRazerKey(Option<u8>),
    Ack,
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_setting_request_round_trips_through_json() {
        let request = IpcRequest::SetPerfMode {
            profile: PowerProfile::Battery,
            mode: PerfMode::Silent,
        };

        let encoded = serde_json::to_string(&request).expect("request must serialize");
        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("request must deserialize");

        assert_eq!(decoded, request);
    }
}
