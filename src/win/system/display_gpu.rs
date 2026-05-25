use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use tracing::{info, warn};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory, IDXGIFactory};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFOEXW, MonitorFromWindow,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, GWLP_USERDATA,
    GetMessageW, GetWindowLongPtrW, MSG, RegisterClassW, SetWindowLongPtrW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_DISPLAYCHANGE, WM_NCCREATE, WNDCLASSW,
};
use windows::core::w;

use crate::utils::reload::restart_app;

/// Detect when main monitor GPU changes and launches app on main display GPU.
/// Ambient Effect causes stutters in games if it runs on iGPU but game and main display are running on dGPU
pub struct GpuDisplayMonitor {
    class_name: windows::core::PCWSTR,
    last_display_gpu: String,
}

impl GpuDisplayMonitor {
    fn new() -> Self {
        Self {
            class_name: w!("GpuMonitorWindowClass"),
            last_display_gpu: String::default(),
        }
    }

    pub fn start() {
        std::thread::spawn(|| {
            let mut monitor = GpuDisplayMonitor::new();
            monitor.run();
        });
    }

    fn run(&mut self) {
        unsafe {
            let wnd_class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(Self::window_procedure),
                hInstance: HINSTANCE::default(),
                lpszClassName: self.class_name,
                ..Default::default()
            };
            RegisterClassW(&wnd_class);

            let _hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                self.class_name,
                w!("GpuMonitorWindow"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                HWND::default(),
                None,
                HINSTANCE::default(),
                Some(self as *mut GpuDisplayMonitor as *const _),
            )
            .expect("Fatal internal error: GpuDisplayMonitor message window");

            // Trigger an explicit baseline check on setup
            self.sync_current_main_display_and_gpu();

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
                // Loop continues executing while window is valid
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
                let create_struct = lparam.0 as *const CREATESTRUCTW;
                if !create_struct.is_null() {
                    let this = (*create_struct).lpCreateParams as isize;
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, this);
                }
            }

            let user_data = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            let monitor: Option<&mut GpuDisplayMonitor> = if user_data != 0 {
                Some(&mut *(user_data as *mut GpuDisplayMonitor)) // ◄ Added Some() here
            } else {
                None
            };

            if msg == WM_DISPLAYCHANGE {
                if let Some(m) = monitor {
                    m.sync_current_main_display_and_gpu();
                }
            }

            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }

    pub unsafe fn sync_current_main_display_and_gpu(&mut self) {
        unsafe {
            let h_monitor = MonitorFromWindow(HWND::default(), MONITOR_DEFAULTTOPRIMARY);

            let mut monitor_info = MONITORINFOEXW::default();
            monitor_info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

            if GetMonitorInfoW(h_monitor, &mut monitor_info.monitorInfo).as_bool() {
                let display_device_name = Self::parse_wchar_slice(&monitor_info.szDevice);
                info!(
                    "Current Main Display : {}",
                    display_device_name.to_string_lossy()
                );

                if let Ok(factory) = CreateDXGIFactory::<IDXGIFactory>() {
                    let mut adapter_index = 0;

                    while let Ok(adapter) = factory.EnumAdapters(adapter_index) {
                        let mut output_index = 0;

                        while let Ok(output) = adapter.EnumOutputs(output_index) {
                            if let Ok(output_desc) = output.GetDesc() {
                                let output_name = Self::parse_wchar_slice(&output_desc.DeviceName);

                                if output_name == display_device_name {
                                    if let Ok(adapter_desc) = adapter.GetDesc() {
                                        let gpu_description =
                                            Self::parse_wchar_slice(&adapter_desc.Description)
                                                .to_string_lossy()
                                                .to_string();

                                        info!("Active GPU Device    : {}", gpu_description);
                                        self.display_gpu_change_callback(gpu_description);
                                        return;
                                    }
                                }
                            }
                            output_index += 1;
                        }
                        adapter_index += 1;
                    }
                    warn!("Active GPU Device    : Unknown (Could not map DXGI topology)");
                } else {
                    eprintln!("Error: Failed to initialize DXGI subsystem.");
                }
            } else {
                eprintln!("Error: Could not retrieve Monitor Info from GDI.");
            }
        }
    }

    fn parse_wchar_slice(slice: &[u16]) -> OsString {
        let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
        OsString::from_wide(&slice[..len])
    }

    fn display_gpu_change_callback(&mut self, new_display_gpu: String) {
        if !self.last_display_gpu.is_empty() && self.last_display_gpu != new_display_gpu {
            info!("Display changed to different graphics adapter. Restarting app...");
            restart_app(1);
        } else {
            self.last_display_gpu = new_display_gpu;
        }
    }
}
