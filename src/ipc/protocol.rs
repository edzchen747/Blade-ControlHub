use serde::{Deserialize, Serialize};

use crate::config::ThemeColor;
use crate::razer::{
    config::PowerProfile,
    enums::{BatteryLimit, PerfMode, RGBEffect},
};
use crate::runtime::settings_state::SettingsState;
use crate::win::system::usbpcap::capture::CapturedCommand;

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
    SetAdvancedExperimentalFeatures {
        enabled: bool,
    },
    SetStartWithAdmin {
        enabled: bool,
    },
    SetStartWithWindows {
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
    BeginCommandLabRecord,
    CancelCommandLabRecord,
    PollCommandLabRecording,
    PlayCommandLabCommands {
        commands: Vec<CapturedCommand>,
    },
    SaveCommandLabCommands {
        name: String,
        commands: Vec<CapturedCommand>,
    },
    RemoveCommandLabCommand { name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandLabStatus {
    Idle,
    Recording,
    Done,
    Cancelled,
    /// Recording could not start (for example without administrator
    /// privileges for the USBPcap capture).
    Failed,
    /// Recording finished but captured more commands than a row can hold;
    /// the capture is discarded and reported as a failure.
    TooManyCommands,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandLabRecordingState {
    pub status: CommandLabStatus,
    pub step: u8,
    pub captured_commands: u32,
    /// The full parsed commands of the finished capture; empty while
    /// recording and for cancelled or never-started recordings.
    pub commands: Vec<CapturedCommand>,
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
    CommandLabRecordingState(CommandLabRecordingState),
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

    #[test]
    fn advanced_experimental_features_request_round_trips_through_json() {
        let request = IpcRequest::SetAdvancedExperimentalFeatures { enabled: false };

        let encoded = serde_json::to_string(&request).expect("request must serialize");
        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("request must deserialize");

        assert_eq!(decoded, request);
    }

    #[test]
    fn start_with_admin_request_round_trips_through_json() {
        let request = IpcRequest::SetStartWithAdmin { enabled: true };

        let encoded = serde_json::to_string(&request).expect("request must serialize");
        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("request must deserialize");

        assert_eq!(decoded, request);
    }

    #[test]
    fn start_with_windows_request_round_trips_through_json() {
        let request = IpcRequest::SetStartWithWindows { enabled: true };

        let encoded = serde_json::to_string(&request).expect("request must serialize");
        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("request must deserialize");

        assert_eq!(decoded, request);
    }

    #[test]
    fn command_lab_recording_state_round_trips_through_json() {
        let state = CommandLabRecordingState {
            status: CommandLabStatus::Recording,
            step: 3,
            captured_commands: 12,
            commands: vec![CapturedCommand {
                command: 0x0303,
                args: vec![0x01, 0x05, 0xFF],
            }],
        };

        let encoded = serde_json::to_string(&state).expect("state must serialize");
        let decoded: CommandLabRecordingState =
            serde_json::from_str(&encoded).expect("state must deserialize");

        assert_eq!(decoded, state);
    }

    #[test]
    fn command_lab_request_round_trips_through_json() {
        let request = IpcRequest::BeginCommandLabRecord;

        let encoded = serde_json::to_string(&request).expect("request must serialize");
        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("request must deserialize");

        assert_eq!(decoded, request);
    }

    #[test]
    fn command_lab_play_and_save_requests_round_trip_through_json() {
        let commands = vec![CapturedCommand {
            command: 0x0303,
            args: vec![0x01, 0x05, 0xFF],
        }];
        for request in [
            IpcRequest::PlayCommandLabCommands {
                commands: commands.clone(),
            },
            IpcRequest::SaveCommandLabCommands {
                name: "Brightness Up".to_owned(),
                commands: commands.clone(),
            },
            IpcRequest::RemoveCommandLabCommand {
                name: "Brightness Up".to_owned(),
            },
        ] {
            let encoded = serde_json::to_string(&request).expect("request must serialize");
            let decoded: IpcRequest =
                serde_json::from_str(&encoded).expect("request must deserialize");
            assert_eq!(decoded, request);
        }
    }
}
