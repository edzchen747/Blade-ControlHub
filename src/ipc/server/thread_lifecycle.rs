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
        let response = IpcResponse::SettingsState(Box::new(expected_state.clone()));
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

        assert_eq!(
            response,
            IpcResponse::SettingsState(Box::new(expected_state))
        );
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
