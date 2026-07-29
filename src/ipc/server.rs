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
        IpcRequest::SetDefaultMultimediaKeys { .. } => "SetDefaultMultimediaKeys",
        IpcRequest::SetPerfMode { .. } => "SetPerfMode",
        IpcRequest::SetRefreshRate { .. } => "SetRefreshRate",
        IpcRequest::SetKeyboardBrightness { .. } => "SetKeyboardBrightness",
        IpcRequest::SetRgbEffect { .. } => "SetRgbEffect",
        IpcRequest::SetUnderGlow { .. } => "SetUnderGlow",
        IpcRequest::SetBatteryLimit { .. } => "SetBatteryLimit",
        IpcRequest::SetThemeColor { .. } => "SetThemeColor",
        IpcRequest::BeginRazerKeyCapture { .. } => "BeginRazerKeyCapture",
        IpcRequest::CancelRazerKeyCapture => "CancelRazerKeyCapture",
        IpcRequest::PollCapturedRazerKey { .. } => "PollCapturedRazerKey",
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
        IpcRequest::BeginRazerKeyCapture { after_unix_ms } => {
            begin_razer_key_capture(after_unix_ms)
        }
        IpcRequest::CancelRazerKeyCapture => cancel_razer_key_capture(),
        IpcRequest::PollCapturedRazerKey { after_sequence } => {
            poll_captured_razer_key(after_sequence)
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

fn razer_key_events() -> MutexGuard<'static, RazerKeyEventLog> {
    RAZER_KEY_EVENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct RazerKeyEventLog {
    next_sequence: u64,
    events: VecDeque<RazerKeyEvent>,
}

impl RazerKeyEventLog {
    const fn new() -> Self {
        Self {
            next_sequence: 0,
            events: VecDeque::new(),
        }
    }

    fn push(&mut self, key_code: u8) -> RazerKeyEvent {
        self.push_at(key_code, current_unix_ms())
    }

    fn push_at(&mut self, key_code: u8, unix_ms: u64) -> RazerKeyEvent {
        self.next_sequence = self.next_sequence.saturating_add(1);
        let event = RazerKeyEvent {
            sequence: self.next_sequence,
            unix_ms,
            key_code,
        };
        self.events.push_back(event);
        while self.events.len() > MAX_RAZER_KEY_EVENTS {
            self.events.pop_front();
        }
        event
    }

    fn latest_sequence_before(&self, unix_ms: u64) -> u64 {
        self.events
            .iter()
            .rev()
            .find(|event| event.unix_ms < unix_ms)
            .map(|event| event.sequence)
            .unwrap_or(0)
    }

    fn first_after(&self, sequence: u64) -> Option<RazerKeyEvent> {
        self.events
            .iter()
            .copied()
            .find(|event| event.sequence > sequence)
    }

    fn clear(&mut self) {
        self.next_sequence = 0;
        self.events.clear();
    }
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
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
    use crate::ipc::framing::{read_json_frame, write_json_frame};
    use crate::razer::config::AppConfig;
    use crate::runtime::settings_state::SettingsState;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_lock() -> MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn record_razer_key_code_records_key_event() {
        let _guard = test_lock();

        razer_key_events().clear();

        let event = record_razer_key_code(0x42);

        assert_eq!(event.key_code, 0x42);
        assert_eq!(razer_key_events().first_after(0), Some(event));
    }

    #[test]
    fn poll_captured_razer_key_stops_listening_after_match() {
        let _guard = test_lock();

        razer_key_events().clear();
        KEYMAP_LISTENING.store(true, Ordering::SeqCst);
        let event = record_razer_key_code(0x42);

        let response = poll_captured_razer_key(0);

        assert_eq!(response, IpcResponse::CapturedRazerKey(Some(event)));
        assert!(!KEYMAP_LISTENING.load(Ordering::SeqCst));
    }

    #[test]
    fn begin_razer_key_capture_returns_current_sequence_baseline() {
        let _guard = test_lock();

        razer_key_events().clear();
        KEYMAP_LISTENING.store(false, Ordering::SeqCst);
        let before_capture = record_razer_key_code(0x41);

        let response = begin_razer_key_capture(before_capture.unix_ms.saturating_add(1));

        assert_eq!(
            response,
            IpcResponse::RazerKeyCaptureStarted {
                after_sequence: before_capture.sequence
            }
        );
        assert!(KEYMAP_LISTENING.load(Ordering::SeqCst));
    }

    #[test]
    fn begin_razer_key_capture_uses_click_time_not_request_arrival_time() {
        let _guard = test_lock();

        razer_key_events().clear();
        let before_click = razer_key_events().push_at(0x40, 100);
        let after_click_before_begin = razer_key_events().push_at(0x41, 200);

        let response = begin_razer_key_capture(150);

        assert_eq!(
            response,
            IpcResponse::RazerKeyCaptureStarted {
                after_sequence: before_click.sequence
            }
        );
        assert_eq!(
            poll_captured_razer_key(before_click.sequence),
            IpcResponse::CapturedRazerKey(Some(after_click_before_begin))
        );
    }

    #[test]
    fn poll_captured_razer_key_ignores_events_before_baseline() {
        let _guard = test_lock();

        razer_key_events().clear();
        let before_capture = record_razer_key_code(0x41);

        let response = poll_captured_razer_key(before_capture.sequence);

        assert_eq!(response, IpcResponse::CapturedRazerKey(None));
    }

    #[test]
    fn join_server_thread_drains_handle() {
        let _guard = test_lock();

        *server_thread() = Some(thread::spawn(|| {}));

        join_server_thread();

        assert!(server_thread().is_none());
    }

    #[test]
    fn ipc_settings_state_response_survives_server_disconnect() {
        let _guard = test_lock();
        let pipe_name = format!(
            r"\\.\pipe\BladeControlHubTest-{}-{}",
            std::process::id(),
            current_unix_ms()
        );
        let expected_state = large_settings_state_for_pipe_test();
        let response = IpcResponse::SettingsState(expected_state.clone());
        let server_pipe_name = pipe_name.clone();

        let server = thread::spawn(move || {
            let pipe = create_pipe_named(&server_pipe_name).expect("test pipe must be created");
            assert!(connect_pipe(&pipe));
            let request = read_json_frame::<IpcRequest>(&pipe).expect("request must be readable");
            assert_eq!(request, IpcRequest::GetSettingsState);
            write_ipc_response(&pipe, &response, Instant::now());
            unsafe {
                let _ = DisconnectNamedPipe(pipe.raw());
            }
        });

        let pipe = connect_test_pipe(&pipe_name);
        write_json_frame(&pipe, &IpcRequest::GetSettingsState).expect("request must write");
        let response = read_json_frame::<IpcResponse>(&pipe).expect("response must read");

        assert_eq!(response, IpcResponse::SettingsState(expected_state));
        server.join().expect("test IPC server must exit");
    }

    fn connect_test_pipe(pipe_name: &str) -> PipeHandle {
        let pipe_name = wide_null(pipe_name);
        let deadline = Instant::now() + Duration::from_secs(1);

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
                Ok(pipe) => return pipe,
                Err(error) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                    if error.kind() == io::ErrorKind::NotFound {
                        continue;
                    }
                }
                Err(error) => panic!("test client failed to connect to pipe: {error}"),
            }
        }
    }

    fn large_settings_state_for_pipe_test() -> SettingsState {
        let mut state = SettingsState::from(AppConfig::default());
        let rates = (0..1_024).map(|idx| 40 + idx).collect::<Vec<_>>();
        state.ac_profile.supported_refresh_rates = rates.clone();
        state.battery_profile.supported_refresh_rates = rates;
        state
    }
}
