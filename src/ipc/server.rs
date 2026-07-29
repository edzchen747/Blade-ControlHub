use std::io;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tracing::warn;
use windows_sys::Win32::Foundation::{ERROR_PIPE_CONNECTED, GetLastError};
use windows_sys::Win32::Security::{
    InitializeSecurityDescriptor, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR,
    SetSecurityDescriptorDacl,
};
use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

use crate::core::shared_state::KEYMAP_LISTENING;
use crate::ipc::framing::{PipeHandle, read_json_frame, wide_null, write_json_frame};
use crate::ipc::protocol::{IpcRequest, IpcResponse, PIPE_NAME};
use crate::razer::device_handle::DeviceHandle;

static IPC_SERVER_RUNNING: AtomicBool = AtomicBool::new(false);
static IPC_SERVER_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static CAPTURED_RAZER_KEY: Mutex<Option<u8>> = Mutex::new(None);

const SECURITY_DESCRIPTOR_REVISION: u32 = 1;

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
    join_server_thread();
}

pub fn capture_razer_key_code(key_code: u8) {
    if !KEYMAP_LISTENING.load(Ordering::SeqCst) {
        return;
    }

    *captured_razer_key() = Some(key_code);
    KEYMAP_LISTENING.store(false, Ordering::SeqCst);
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
    *captured_razer_key() = None;
    IPC_SERVER_RUNNING.store(false, Ordering::SeqCst);
}

fn handle_client(pipe: &PipeHandle, device: &DeviceHandle) {
    let response = match read_json_frame::<IpcRequest>(pipe) {
        Ok(request) => dispatch_request(request, device),
        Err(error) => IpcResponse::Error {
            message: format!("Failed to read IPC request: {error}"),
        },
    };

    if let Err(error) = write_json_frame(pipe, &response) {
        warn!(%error, "Failed to write IPC response");
    }
}

fn dispatch_request(request: IpcRequest, device: &DeviceHandle) -> IpcResponse {
    match request {
        IpcRequest::GetSettingsState => match device.get_settings_state() {
            Ok(state) => IpcResponse::SettingsState(state),
            Err(error) => IpcResponse::Error {
                message: error.to_string(),
            },
        },
        IpcRequest::SetDefaultMultimediaKeys { enabled } => {
            ack(device.set_default_multimedia_keys(enabled))
        }
        IpcRequest::SetPerfMode { profile, mode } => ack(device.set_perf_mode(profile, mode)),
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
        IpcRequest::BeginRazerKeyCapture => {
            *captured_razer_key() = None;
            KEYMAP_LISTENING.store(true, Ordering::SeqCst);
            IpcResponse::Ack
        }
        IpcRequest::CancelRazerKeyCapture => {
            KEYMAP_LISTENING.store(false, Ordering::SeqCst);
            *captured_razer_key() = None;
            IpcResponse::Ack
        }
        IpcRequest::PollCapturedRazerKey => {
            IpcResponse::CapturedRazerKey(captured_razer_key().take())
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

fn create_pipe() -> io::Result<PipeHandle> {
    let pipe_name = wide_null(PIPE_NAME);
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

struct PipeSecurity {
    descriptor: SECURITY_DESCRIPTOR,
    attributes: SECURITY_ATTRIBUTES,
}

impl PipeSecurity {
    fn new() -> io::Result<Self> {
        let mut descriptor = unsafe { std::mem::zeroed::<SECURITY_DESCRIPTOR>() };
        let initialized = unsafe {
            InitializeSecurityDescriptor(
                (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                SECURITY_DESCRIPTOR_REVISION,
            )
        };
        if initialized == 0 {
            return Err(io::Error::last_os_error());
        }

        let dacl_set = unsafe {
            SetSecurityDescriptorDacl(
                (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                1,
                null_mut(),
                0,
            )
        };
        if dacl_set == 0 {
            return Err(io::Error::last_os_error());
        }

        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
            bInheritHandle: 0,
        };

        Ok(Self {
            descriptor,
            attributes,
        })
    }

    fn as_mut_ptr(&mut self) -> *mut SECURITY_ATTRIBUTES {
        self.attributes.lpSecurityDescriptor =
            (&mut self.descriptor as *mut SECURITY_DESCRIPTOR).cast();
        &mut self.attributes
    }
}

fn connect_pipe(pipe: &PipeHandle) -> bool {
    let ok = unsafe { ConnectNamedPipe(pipe.raw(), null_mut()) };
    ok != 0 || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED
}

fn server_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    IPC_SERVER_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn captured_razer_key() -> MutexGuard<'static, Option<u8>> {
    CAPTURED_RAZER_KEY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_server_thread() {
    let should_join = server_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);

    if should_join {
        join_server_thread();
    }
}

fn join_server_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = server_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current IPC server thread during shutdown");
        *server_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("IPC server thread panicked during shutdown");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_lock() -> MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn capture_razer_key_code_records_key_and_stops_listening() {
        let _guard = test_lock();

        KEYMAP_LISTENING.store(true, Ordering::SeqCst);
        *captured_razer_key() = None;

        capture_razer_key_code(0x42);

        assert!(!KEYMAP_LISTENING.load(Ordering::SeqCst));
        assert_eq!(captured_razer_key().take(), Some(0x42));
    }

    #[test]
    fn capture_razer_key_code_ignores_keys_when_not_listening() {
        let _guard = test_lock();

        KEYMAP_LISTENING.store(false, Ordering::SeqCst);
        *captured_razer_key() = None;

        capture_razer_key_code(0x42);

        assert_eq!(captured_razer_key().take(), None);
    }

    #[test]
    fn join_server_thread_drains_handle() {
        let _guard = test_lock();

        *server_thread() = Some(thread::spawn(|| {}));

        join_server_thread();

        assert!(server_thread().is_none());
    }
}
