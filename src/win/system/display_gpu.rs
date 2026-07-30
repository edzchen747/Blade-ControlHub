use crate::razer::device_handle::device;
use crate::win::display::topology::{
    DisplayLayout, current_display_layout, display_layout_changed, primary_display_device_name,
    wide_slice_to_os_string,
};
use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use tracing::{error, info, warn};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory, IDXGIFactory};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
    DispatchMessageW, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, MSG, PostMessageW,
    RegisterClassW, SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_DISPLAYCHANGE, WM_NCCREATE, WM_USER, WNDCLASSW,
};
use windows::core::{PCWSTR, Result as WinResult, w};

const WM_GPU_MONITOR_STOP: u32 = WM_USER + 3;
const WM_GPU_MONITOR_DISPLAY_CHANGE: u32 = WM_USER + 4;
static GPU_MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);
static GPU_MONITOR_HWND: AtomicIsize = AtomicIsize::new(0);
static GPU_MONITOR_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Detect when main monitor GPU changes and launches app on main display GPU.
/// Ambient Effect causes stutters in games if it runs on iGPU but game and main display are running on dGPU
pub struct GpuDisplayMonitor {
    class_name: PCWSTR,
    last_display_gpu: String,
    last_display_layout: Option<DisplayLayout>,
}

impl GpuDisplayMonitor {
    fn new() -> Self {
        Self {
            class_name: w!("GpuMonitorWindowClass"),
            last_display_gpu: String::default(),
            last_display_layout: None,
        }
    }

    pub fn start() {
        join_finished_gpu_monitor_thread();

        if GPU_MONITOR_RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }

        match thread::Builder::new()
            .name("blade-gpu-display-monitor".to_string())
            .spawn(|| {
                let mut monitor = GpuDisplayMonitor::new();
                monitor.run();
            }) {
            Ok(handle) => {
                *gpu_monitor_thread() = Some(handle);
            }
            Err(error) => {
                GPU_MONITOR_RUNNING.store(false, Ordering::SeqCst);
                error!(%error, "Failed to start GPU display monitor thread");
            }
        }
    }

    pub fn stop() {
        GPU_MONITOR_RUNNING.store(false, Ordering::SeqCst);
        wake_gpu_monitor(WM_GPU_MONITOR_STOP);
        join_gpu_monitor_thread();
    }

    pub fn trigger_display_change() {
        if !wake_gpu_monitor(WM_GPU_MONITOR_DISPLAY_CHANGE) {
            device().display_layout_changed();
        }
    }

    fn run(&mut self) {
        let Some(window) = GpuMonitorWindow::create(self) else {
            GPU_MONITOR_RUNNING.store(false, Ordering::SeqCst);
            return;
        };
        GPU_MONITOR_HWND.store(window.hwnd.0 as isize, Ordering::SeqCst);

        // Trigger an explicit baseline check on setup.
        self.refresh_display_layout_baseline();
        self.sync_current_main_display_and_gpu();
        run_gpu_monitor_message_loop();
        GPU_MONITOR_HWND.store(0, Ordering::SeqCst);
        GPU_MONITOR_RUNNING.store(false, Ordering::SeqCst);
    }

    pub fn sync_current_main_display_and_gpu(&mut self) {
        let Some(display_device_name) = primary_display_device_name() else {
            error!("Could not retrieve monitor info from GDI");
            return;
        };

        info!(
            "Current Main Display : {}",
            display_device_name.to_string_lossy()
        );

        match active_gpu_for_display(&display_device_name) {
            Ok(Some(gpu_description)) => {
                info!("Active GPU Device    : {}", gpu_description);
                self.display_gpu_change_callback(gpu_description);
            }
            Ok(None) => {
                warn!("Active GPU Device    : Unknown (Could not map DXGI topology)");
            }
            Err(error) => {
                error!(?error, "Failed to initialize DXGI subsystem");
            }
        }
    }

    /// The window callback procedure conforming exactly to native Win32 requirements.
    unsafe extern "system" fn window_procedure(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe {
            if msg == WM_NCCREATE {
                store_monitor_pointer(hwnd, lparam);
            };

            if GPU_MONITOR_RUNNING.load(Ordering::SeqCst) && msg == WM_DISPLAYCHANGE {
                with_monitor(hwnd, |monitor| monitor.process_display_change(false));
            }

            if GPU_MONITOR_RUNNING.load(Ordering::SeqCst) && msg == WM_GPU_MONITOR_DISPLAY_CHANGE {
                with_monitor(hwnd, |monitor| monitor.process_display_change(true));
            }

            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }

    fn display_gpu_change_callback(&mut self, new_display_gpu: String) {
        if display_gpu_changed(&self.last_display_gpu, &new_display_gpu) {
            info!("Display changed to different graphics adapter; restart is no longer required");
        }
        self.last_display_gpu = new_display_gpu;
    }

    fn refresh_display_layout_baseline(&mut self) {
        self.last_display_layout = Some(current_display_layout());
    }

    fn process_display_change(&mut self, force: bool) {
        let current_layout = current_display_layout();
        let changed =
            force || display_layout_changed(self.last_display_layout.as_ref(), &current_layout);
        self.last_display_layout = Some(current_layout);

        if changed {
            info!(
                force,
                "Display layout changed; refreshing display-dependent runtime state"
            );
            device().display_layout_changed();
            self.sync_current_main_display_and_gpu();
        }
    }
}

fn gpu_monitor_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    GPU_MONITOR_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_gpu_monitor_thread() {
    let should_join = gpu_monitor_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);

    if should_join {
        join_gpu_monitor_thread();
    }
}

fn join_gpu_monitor_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = gpu_monitor_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current GPU display monitor thread during shutdown");
        *gpu_monitor_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("GPU display monitor thread panicked during shutdown");
    }
}

fn wake_gpu_monitor(message: u32) -> bool {
    let hwnd = HWND(GPU_MONITOR_HWND.load(Ordering::SeqCst) as *mut _);
    if hwnd.0.is_null() {
        return false;
    }

    unsafe { PostMessageW(hwnd, message, WPARAM(0), LPARAM(0)) }.is_ok()
}

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
