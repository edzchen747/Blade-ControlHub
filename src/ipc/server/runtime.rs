use std::collections::VecDeque;
use std::io;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tracing::{debug, info, warn};
use windows_sys::Win32::Foundation::{ERROR_PIPE_CONNECTED, GetLastError};
use windows_sys::Win32::Security::{
    InitializeSecurityDescriptor, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR,
    SetSecurityDescriptorDacl,
};
use windows_sys::Win32::Storage::FileSystem::{FlushFileBuffers, PIPE_ACCESS_DUPLEX};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

use crate::core::shared_state::KEYMAP_LISTENING;
use crate::ipc::framing::{PipeHandle, read_json_frame, wide_null, write_json_frame};
use crate::ipc::protocol::{IpcRequest, IpcResponse, PIPE_NAME, RazerKeyEvent};
use crate::razer::device_handle::DeviceHandle;

static IPC_SERVER_RUNNING: AtomicBool = AtomicBool::new(false);
static IPC_SERVER_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static RAZER_KEY_EVENTS: Mutex<RazerKeyEventLog> = Mutex::new(RazerKeyEventLog::new());

const SECURITY_DESCRIPTOR_REVISION: u32 = 1;
const MAX_RAZER_KEY_EVENTS: usize = 32;

pub fn start(device: DeviceHandle) {
    join_finished_server_thread();

    if IPC_SERVER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    match thread::Builder::new()
        .name("blade-ipc-server".to_string())
        .spawn(move || run_server(device))
    {
        Ok(handle) => {
            *server_thread() = Some(handle);
        }
        Err(error) => {
            IPC_SERVER_RUNNING.store(false, Ordering::SeqCst);
            warn!(%error, "Failed to start IPC server thread");
        }
    }
}

pub fn stop() {
    IPC_SERVER_RUNNING.store(false, Ordering::SeqCst);
    let _ = crate::ipc::client::send_request(IpcRequest::CancelRazerKeyCapture);
    cancel_command_lab_record();
    join_server_thread();
}

pub fn record_razer_key_code(key_code: u8) -> RazerKeyEvent {
    razer_key_events().push(key_code)
}

fn run_server(device: DeviceHandle) {
    while IPC_SERVER_RUNNING.load(Ordering::SeqCst) {
        match create_pipe() {
            Ok(pipe) => {
                if connect_pipe(&pipe) {
                    handle_client(&pipe, &device);
                }
                unsafe {
                    let _ = DisconnectNamedPipe(pipe.raw());
                }
            }
            Err(error) => {
                warn!(%error, "Failed to create IPC named pipe");
                thread::sleep(Duration::from_millis(250));
            }
        }
    }

    KEYMAP_LISTENING.store(false, Ordering::SeqCst);
    razer_key_events().clear();
    IPC_SERVER_RUNNING.store(false, Ordering::SeqCst);
}

fn handle_client(pipe: &PipeHandle, device: &DeviceHandle) {
    let started = Instant::now();
    let response = match read_json_frame::<IpcRequest>(pipe) {
        Ok(request) => {
            let request_kind = ipc_request_kind(&request);
            debug!(request = request_kind, "Received IPC request");
            let response = dispatch_request(request, device);
            info!(
                request = request_kind,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Handled IPC request"
            );
            response
        }
        Err(error) => {
            warn!(
                %error,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Failed to read IPC request"
            );
            IpcResponse::Error {
                message: format!("Failed to read IPC request: {error}"),
            }
        }
    };

    write_ipc_response(pipe, &response, started);
}

fn write_ipc_response(pipe: &PipeHandle, response: &IpcResponse, started: Instant) {
    match write_json_frame(pipe, response) {
        Ok(()) => {
            let flushed = unsafe { FlushFileBuffers(pipe.raw()) };
            if flushed == 0 {
                warn!(
                    error = %io::Error::last_os_error(),
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "Failed to flush IPC response"
                );
            }
        }
        Err(error) => {
            warn!(
                %error,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Failed to write IPC response"
            );
        }
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

fn dispatch_request(request: IpcRequest, device: &DeviceHandle) -> IpcResponse {
    match request {
        IpcRequest::GetSettingsState => match device.get_settings_state() {
            Ok(state) => IpcResponse::SettingsState(Box::new(state)),
            Err(error) => IpcResponse::Error {
                message: error.to_string(),
            },
        },
        IpcRequest::SetSettingsWindowOpen { open, focused } => {
            crate::ui::app::set_settings_window_state(open, focused);
            IpcResponse::Ack
        }
        IpcRequest::SetPrimaryMultimediaKeys { enabled } => {
            ack(device.set_primary_multimedia_keys(enabled))
        }
        IpcRequest::SetAdvancedExperimentalFeatures { enabled } => {
            ack(device.set_advanced_experimental_features(enabled))
        }
        IpcRequest::SetPerfMode { profile, mode } => ack(device.set_perf_mode(profile, mode)),
        IpcRequest::SetCustomModeConfig {
            cpu_level,
            gpu_level,
        } => ack(device.set_custom_mode_config(cpu_level, gpu_level)),
        IpcRequest::SetFanSpeed { profile, speed } => ack(device.set_fan_speed(profile, speed)),
        IpcRequest::SetRefreshRate { profile, hz } => ack(device.set_refresh_rate(profile, hz)),
        IpcRequest::SetKeyboardBrightness { profile, level } => {
            ack(device.set_keyboard_brightness(profile, level))
        }
        IpcRequest::SetRgbEffect { profile, effect } => ack(device.set_rgb_mode(profile, effect)),
        IpcRequest::SetUnderGlow { profile, enabled } => {
            ack(device.set_under_glow(profile, enabled))
        }
        IpcRequest::SetBatteryLimit { limit } => ack(device.set_battery_limit(limit)),
        IpcRequest::SetThemeColor { color } => ack(device.set_theme_color(color)),
        IpcRequest::BeginRazerKeyCapture { after_unix_ms } => {
            begin_razer_key_capture(after_unix_ms)
        }
        IpcRequest::CancelRazerKeyCapture => cancel_razer_key_capture(),
        IpcRequest::PollCapturedRazerKey { after_sequence } => {
            poll_captured_razer_key(after_sequence)
        }
        IpcRequest::BeginCommandLabRecord => {
            begin_command_lab_record();
            IpcResponse::Ack
        }
        IpcRequest::CancelCommandLabRecord => {
            cancel_command_lab_record();
            IpcResponse::Ack
        }
        IpcRequest::PollCommandLabRecording => {
            IpcResponse::CommandLabRecordingState(poll_command_lab_recording())
        }
    }
}

fn ack(result: crate::error::AppResult<()>) -> IpcResponse {
    match result {
        Ok(()) => IpcResponse::Ack,
        Err(error) => IpcResponse::Error {
            message: error.to_string(),
        },
    }
}

fn begin_razer_key_capture(after_unix_ms: u64) -> IpcResponse {
    let events = razer_key_events();
    let after_sequence = events.latest_sequence_before(after_unix_ms);
    KEYMAP_LISTENING.store(true, Ordering::SeqCst);
    IpcResponse::RazerKeyCaptureStarted { after_sequence }
}

fn cancel_razer_key_capture() -> IpcResponse {
    KEYMAP_LISTENING.store(false, Ordering::SeqCst);
    IpcResponse::Ack
}

fn poll_captured_razer_key(after_sequence: u64) -> IpcResponse {
    let event = razer_key_events().first_after(after_sequence);
    if event.is_some() {
        KEYMAP_LISTENING.store(false, Ordering::SeqCst);
    }
    IpcResponse::CapturedRazerKey(event)
}

fn create_pipe() -> io::Result<PipeHandle> {
    create_pipe_named(PIPE_NAME)
}

fn create_pipe_named(name: &str) -> io::Result<PipeHandle> {
    let pipe_name = wide_null(name);
    let mut pipe_security = PipeSecurity::new()?;
    let handle = unsafe {
        CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            64 * 1024,
            64 * 1024,
            0,
            pipe_security.as_mut_ptr(),
        )
    };
    PipeHandle::new(handle)
}

