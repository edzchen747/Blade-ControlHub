struct GpuMonitorWindow {
    hwnd: HWND,
}

impl GpuMonitorWindow {
    fn create(monitor: &mut GpuDisplayMonitor) -> Option<Self> {
        create_monitor_window(monitor).map(|hwnd| Self { hwnd })
    }
}

impl Drop for GpuMonitorWindow {
    fn drop(&mut self) {
        if self.hwnd.0.is_null() {
            return;
        }

        if let Err(error) = unsafe { DestroyWindow(self.hwnd) } {
            warn!(?error, "Failed to destroy GPU display monitor window");
        }
    }
}

fn create_monitor_window(monitor: &mut GpuDisplayMonitor) -> Option<HWND> {
    unsafe {
        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(GpuDisplayMonitor::window_procedure),
            hInstance: HINSTANCE::default(),
            lpszClassName: monitor.class_name,
            ..Default::default()
        };
        let _ = RegisterClassW(&wnd_class);

        match CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            monitor.class_name,
            w!("GpuMonitorWindow"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            HWND::default(),
            None,
            HINSTANCE::default(),
            Some(monitor as *mut GpuDisplayMonitor as *const _),
        ) {
            Ok(hwnd) => Some(hwnd),
            Err(error) => {
                error!(
                    ?error,
                    "Failed to create GPU display monitor message window"
                );
                None
            }
        }
    }
}

fn run_gpu_monitor_message_loop() {
    unsafe {
        let mut msg = MSG::default();
        while GPU_MONITOR_RUNNING.load(Ordering::SeqCst)
            && GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool()
        {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);

            if msg.message == WM_GPU_MONITOR_STOP {
                break;
            }
        }
    }
}

fn store_monitor_pointer(hwnd: HWND, lparam: LPARAM) {
    let create_struct = lparam.0 as *const CREATESTRUCTW;
    if !create_struct.is_null() {
        let this = unsafe { (*create_struct).lpCreateParams as isize };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, this) };
    }
}

fn with_monitor(hwnd: HWND, f: impl FnOnce(&mut GpuDisplayMonitor)) {
    let user_data = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    if user_data != 0 {
        let monitor = unsafe { &mut *(user_data as *mut GpuDisplayMonitor) };
        f(monitor);
    }
}

fn active_gpu_for_display(display_device_name: &OsString) -> WinResult<Option<String>> {
    let factory = unsafe { CreateDXGIFactory::<IDXGIFactory>()? };
    let mut adapter_index = 0;

    while let Ok(adapter) = unsafe { factory.EnumAdapters(adapter_index) } {
        let mut output_index = 0;

        while let Ok(output) = unsafe { adapter.EnumOutputs(output_index) } {
            if let Ok(output_desc) = unsafe { output.GetDesc() } {
                let output_name = wide_slice_to_os_string(&output_desc.DeviceName);

                if &output_name != display_device_name {
                    continue;
                }

                if let Ok(adapter_desc) = unsafe { adapter.GetDesc() } {
                    return Ok(Some(
                        wide_slice_to_os_string(&adapter_desc.Description)
                            .to_string_lossy()
                            .to_string(),
                    ));
                }
            }
            output_index += 1;
        }
        adapter_index += 1;
    }

    Ok(None)
}

fn display_gpu_changed(last_display_gpu: &str, new_display_gpu: &str) -> bool {
    !last_display_gpu.is_empty() && last_display_gpu != new_display_gpu
}

#[cfg(test)]
mod tests {
    use super::{
        GPU_MONITOR_HWND, GPU_MONITOR_RUNNING, GpuDisplayMonitor, gpu_monitor_thread,
        join_gpu_monitor_thread,
    };
    use crate::win::display::topology::wide_slice_to_os_string;
    use std::sync::atomic::Ordering;
    use std::thread;

    #[test]
    fn parse_wchar_slice_stops_at_nul() {
        let wide = [
            'D' as u16, 'G' as u16, 'P' as u16, 'U' as u16, 0, 'X' as u16,
        ];

        assert_eq!(wide_slice_to_os_string(&wide).to_string_lossy(), "DGPU");
    }

    #[test]
    fn gpu_change_is_ignored_before_baseline() {
        assert!(!super::display_gpu_changed("", "NVIDIA"));
    }

    #[test]
    fn gpu_change_is_false_for_same_gpu() {
        assert!(!super::display_gpu_changed("NVIDIA", "NVIDIA"));
    }

    #[test]
    fn gpu_change_is_detected_after_baseline_changes() {
        assert!(super::display_gpu_changed("Intel", "NVIDIA"));
    }

    #[test]
    fn gpu_change_callback_updates_last_gpu_without_restart() {
        let mut monitor = GpuDisplayMonitor::new();

        monitor.display_gpu_change_callback("Intel".to_string());
        monitor.display_gpu_change_callback("NVIDIA".to_string());

        assert_eq!(monitor.last_display_gpu, "NVIDIA");
    }

    #[test]
    fn stop_clears_gpu_monitor_running_flag() {
        GPU_MONITOR_RUNNING.store(true, Ordering::SeqCst);
        GPU_MONITOR_HWND.store(0, Ordering::SeqCst);

        GpuDisplayMonitor::stop();

        assert!(!GPU_MONITOR_RUNNING.load(Ordering::SeqCst));
        assert_eq!(GPU_MONITOR_HWND.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn join_gpu_monitor_thread_drains_handle() {
        *gpu_monitor_thread() = Some(thread::spawn(|| {}));

        join_gpu_monitor_thread();

        assert!(gpu_monitor_thread().is_none());
    }
}
