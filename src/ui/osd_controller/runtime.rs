fn osd_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    OSD_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_osd_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = osd_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current OSD window thread during shutdown");
        *osd_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("OSD window thread panicked during shutdown");
    }
}

// --- Shared Utility Implementations ---

fn post_osd_update(hwnd: HWND, params: OsdParams) {
    unsafe {
        let boxed_params = Box::new(params);
        let lparam = LPARAM(Box::into_raw(boxed_params) as isize);

        if PostMessageW(hwnd, WM_TRIGGER_OSD, WPARAM(0), lparam).is_err() {
            // Prevent memory leaks if the target window receiver pipeline goes offline.
            let _ = Box::from_raw(lparam.0 as *mut OsdParams);
        }
    }
}

fn post_osd_stop(hwnd: HWND) {
    let _ = unsafe { PostMessageW(hwnd, WM_STOP_OSD, WPARAM(0), LPARAM(0)) };
}

fn run_osd_window_thread(tx: Sender<Option<SendableHwnd>>) {
    let _ = get_svg_options();

    let hwnd = match create_osd_window() {
        Some(hwnd) => hwnd,
        None => {
            let _ = tx.send(None);
            return;
        }
    };

    let state = Box::new(OsdWindowState::default());
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
    }

    center_window(hwnd);
    let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };

    OSD_RUNNING.store(true, Ordering::SeqCst);
    let _ = tx.send(Some(SendableHwnd(hwnd)));
    run_osd_message_loop();
    OSD_RUNNING.store(false, Ordering::SeqCst);
}

fn create_osd_window() -> Option<HWND> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let class_name = to_wstring("RustOSD_SVG");
        let title = to_wstring("Rust OSD");
        let instance: HINSTANCE = match GetModuleHandleW(None) {
            Ok(handle) => handle.into(),
            Err(error) => {
                warn!(?error, "Failed to acquire module handle for OSD window");
                return None;
            }
        };

        let wc = WNDCLASSW {
            lpfnWndProc: Some(OsdController::window_proc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };

        RegisterClassW(&wc);

        match CreateWindowExW(
            OSD_EX_STYLE,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            0,
            0,
            BASE_SIZE as i32,
            BASE_SIZE as i32,
            None,
            None,
            instance,
            None,
        ) {
            Ok(hwnd) => Some(hwnd),
            Err(error) => {
                warn!(?error, "Failed to create OSD layered window");
                None
            }
        }
    }
}

fn run_osd_message_loop() {
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn to_wstring(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn with_osd_state<R>(hwnd: HWND, action: impl FnOnce(&mut OsdWindowState) -> R) -> Option<R> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OsdWindowState };
    if ptr.is_null() {
        return None;
    }

    // SAFETY: The pointer is installed from a Box in `run_osd_window_thread`
    // and cleared in `drop_osd_state`. OSD window state is only accessed from
    // this window procedure on the dedicated OSD thread.
    Some(action(unsafe { &mut *ptr }))
}

fn drop_osd_state(hwnd: HWND) {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OsdWindowState };
    if !ptr.is_null() {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(ptr));
        }
    }
}

