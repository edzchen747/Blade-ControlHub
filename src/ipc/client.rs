use std::io;
use std::ptr::null_mut;
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};

use crate::config::ThemeColor;
use crate::error::{AppError, AppResult};
use crate::ipc::framing::{PipeHandle, read_json_frame, wide_null, write_json_frame};
use crate::ipc::protocol::{
    CommandLabRecordingState, IpcRequest, IpcResponse, PIPE_NAME, RazerKeyEvent,
};
use crate::razer::{
    config::PowerProfile,
    enums::{BatteryLimit, PerfMode, RGBEffect},
};
use crate::runtime::settings_state::SettingsState;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const CONNECT_RETRY: Duration = Duration::from_millis(25);

pub fn get_settings_state() -> AppResult<SettingsState> {
    match send_request(IpcRequest::GetSettingsState)? {
        IpcResponse::SettingsState(state) => Ok(*state),
        response => Err(unexpected_response(response)),
    }
}

pub fn set_settings_window_state(open: bool, focused: bool) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetSettingsWindowOpen {
        open,
        focused,
    })?)
}

pub fn set_primary_multimedia_keys(enabled: bool) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetPrimaryMultimediaKeys {
        enabled,
    })?)
}

pub fn set_advanced_experimental_features(enabled: bool) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetAdvancedExperimentalFeatures {
        enabled,
    })?)
}

pub fn set_perf_mode(profile: PowerProfile, mode: PerfMode) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetPerfMode { profile, mode })?)
}

pub fn set_custom_mode_config(cpu_level: u8, gpu_level: u8) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetCustomModeConfig {
        cpu_level,
        gpu_level,
    })?)
}

pub fn set_fan_speed(profile: PowerProfile, speed: u8) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetFanSpeed { profile, speed })?)
}

pub fn set_refresh_rate(profile: PowerProfile, hz: u32) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetRefreshRate { profile, hz })?)
}

pub fn set_keyboard_brightness(profile: PowerProfile, level: u8) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetKeyboardBrightness {
        profile,
        level,
    })?)
}

pub fn set_rgb_effect(profile: PowerProfile, effect: RGBEffect) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetRgbEffect { profile, effect })?)
}

pub fn set_under_glow(profile: PowerProfile, enabled: bool) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetUnderGlow { profile, enabled })?)
}

pub fn set_battery_limit(limit: BatteryLimit) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetBatteryLimit { limit })?)
}

pub fn set_theme_color(color: ThemeColor) -> AppResult<()> {
    expect_ack(send_request(IpcRequest::SetThemeColor { color })?)
}

pub fn begin_razer_key_capture(after_unix_ms: u64) -> AppResult<u64> {
    match send_request(IpcRequest::BeginRazerKeyCapture { after_unix_ms })? {
        IpcResponse::RazerKeyCaptureStarted { after_sequence } => Ok(after_sequence),
        response => Err(unexpected_response(response)),
    }
}

pub fn cancel_razer_key_capture() -> AppResult<()> {
    expect_ack(send_request(IpcRequest::CancelRazerKeyCapture)?)
}

pub fn poll_captured_razer_key(after_sequence: u64) -> AppResult<Option<RazerKeyEvent>> {
    match send_request(IpcRequest::PollCapturedRazerKey { after_sequence })? {
        IpcResponse::CapturedRazerKey(event) => Ok(event),
        response => Err(unexpected_response(response)),
    }
}

pub fn begin_command_lab_record() -> AppResult<()> {
    expect_ack(send_request(IpcRequest::BeginCommandLabRecord)?)
}

pub fn cancel_command_lab_record() -> AppResult<()> {
    expect_ack(send_request(IpcRequest::CancelCommandLabRecord)?)
}

pub fn poll_command_lab_recording() -> AppResult<CommandLabRecordingState> {
    match send_request(IpcRequest::PollCommandLabRecording)? {
        IpcResponse::CommandLabRecordingState(state) => Ok(state),
        response => Err(unexpected_response(response)),
    }
}

pub fn send_request(request: IpcRequest) -> AppResult<IpcResponse> {
    let started = Instant::now();
    let request_kind = ipc_request_kind(&request);
    debug!(request = request_kind, "Sending IPC request");

    let connect_started = Instant::now();
    let pipe = match connect() {
        Ok(pipe) => {
            debug!(
                request = request_kind,
                elapsed_ms = connect_started.elapsed().as_millis() as u64,
                "Connected IPC pipe"
            );
            pipe
        }
        Err(error) => {
            warn!(
                request = request_kind,
                elapsed_ms = connect_started.elapsed().as_millis() as u64,
                "Failed to connect IPC pipe"
            );
            return Err(error.into());
        }
    };

    if let Err(error) = write_json_frame(&pipe, &request) {
        warn!(
            request = request_kind,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "Failed to write IPC request"
        );
        return Err(error.into());
    }

    let response = match read_json_frame(&pipe) {
        Ok(response) => response,
        Err(error) => {
            warn!(
                request = request_kind,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Failed to read IPC response"
            );
            return Err(error.into());
        }
    };

    info!(
        request = request_kind,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "Completed IPC request"
    );

    match response {
        IpcResponse::Error { message } => Err(AppError::Internal(message)),
        response => Ok(response),
    }
}

fn ipc_request_kind(request: &IpcRequest) -> &'static str {
    match request {
        IpcRequest::GetSettingsState => "GetSettingsState",
        IpcRequest::SetSettingsWindowOpen { .. } => "SetSettingsWindowOpen",
        IpcRequest::SetPrimaryMultimediaKeys { .. } => "SetPrimaryMultimediaKeys",
        IpcRequest::SetAdvancedExperimentalFeatures { .. } => "SetAdvancedExperimentalFeatures",
        IpcRequest::SetPerfMode { .. } => "SetPerfMode",
        IpcRequest::SetCustomModeConfig { .. } => "SetCustomModeConfig",
        IpcRequest::SetFanSpeed { .. } => "SetFanSpeed",
        IpcRequest::SetRefreshRate { .. } => "SetRefreshRate",
        IpcRequest::SetKeyboardBrightness { .. } => "SetKeyboardBrightness",
        IpcRequest::SetRgbEffect { .. } => "SetRgbEffect",
        IpcRequest::SetUnderGlow { .. } => "SetUnderGlow",
        IpcRequest::SetBatteryLimit { .. } => "SetBatteryLimit",
        IpcRequest::SetThemeColor { .. } => "SetThemeColor",
        IpcRequest::BeginRazerKeyCapture { .. } => "BeginRazerKeyCapture",
        IpcRequest::CancelRazerKeyCapture => "CancelRazerKeyCapture",
        IpcRequest::PollCapturedRazerKey { .. } => "PollCapturedRazerKey",
        IpcRequest::BeginCommandLabRecord => "BeginCommandLabRecord",
        IpcRequest::CancelCommandLabRecord => "CancelCommandLabRecord",
        IpcRequest::PollCommandLabRecording => "PollCommandLabRecording",
    }
}

fn expect_ack(response: IpcResponse) -> AppResult<()> {
    match response {
        IpcResponse::Ack => Ok(()),
        response => Err(unexpected_response(response)),
    }
}

fn unexpected_response(response: IpcResponse) -> AppError {
    AppError::Internal(format!("Unexpected IPC response: {response:?}"))
}

fn connect() -> io::Result<PipeHandle> {
    let pipe_name = wide_null(PIPE_NAME);
    let deadline = Instant::now() + CONNECT_TIMEOUT;

    loop {
        let handle = unsafe {
            CreateFileW(
                pipe_name.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                0,
            )
        };

        match PipeHandle::new(handle) {
            Ok(pipe) => return Ok(pipe),
            Err(error)
                if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
                    && Instant::now() < deadline =>
            {
                thread::sleep(CONNECT_RETRY);
            }
            Err(error) if Instant::now() < deadline => {
                thread::sleep(CONNECT_RETRY);
                if error.kind() == io::ErrorKind::NotFound {
                    continue;
                }
            }
            Err(error) => return Err(error),
        }
    }
}
