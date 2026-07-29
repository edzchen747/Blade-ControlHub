use serde::{Deserialize, Serialize};

use crate::runtime::settings_state::SettingsState;

pub const PIPE_NAME: &str = r"\\.\pipe\BladeControlHub";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcRequest {
    GetSettingsState,
    SetDefaultMultimediaKeys { enabled: bool },
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
