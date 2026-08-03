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
    SetSettingsWindowOpen {
        open: bool,
        focused: bool,
    },
    SetPrimaryMultimediaKeys {
        enabled: bool,
    },
    SetPerfMode {
        profile: PowerProfile,
        mode: PerfMode,
    },
    SetCustomModeConfig {
        cpu_level: u8,
        gpu_level: u8,
    },
    SetFanSpeed {
        profile: PowerProfile,
        speed: u8,
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
    BeginRazerKeyCapture {
        after_unix_ms: u64,
    },
    CancelRazerKeyCapture,
    PollCapturedRazerKey {
        after_sequence: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RazerKeyEvent {
    pub sequence: u64,
    pub unix_ms: u64,
    pub key_code: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcResponse {
    SettingsState(Box<SettingsState>),
    RazerKeyCaptureStarted { after_sequence: u64 },
    CapturedRazerKey(Option<RazerKeyEvent>),
    Ack,
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_setting_request_round_trips_through_json() {
        let request = IpcRequest::SetFanSpeed {
            profile: PowerProfile::Battery,
            speed: 46,
        };

        let encoded = serde_json::to_string(&request).expect("request must serialize");
        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("request must deserialize");

        assert_eq!(decoded, request);
    }

    #[test]
    fn settings_window_state_request_round_trips_through_json() {
        let request = IpcRequest::SetSettingsWindowOpen {
            open: true,
            focused: true,
        };

        let encoded = serde_json::to_string(&request).expect("request must serialize");
        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("request must deserialize");

        assert_eq!(decoded, request);
    }
}
