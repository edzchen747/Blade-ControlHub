use crate::razer;

use std::ptr::null_mut;
use std::sync::{Arc, Mutex};
use std::thread;
use tracing::{error, info, warn};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{
    DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, RegisterSuspendResumeNotification,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DEVICE_NOTIFY_CALLBACK, DefWindowProcW, DispatchMessageW, GetMessageW, MSG,
    PostMessageW, RegisterClassW, TranslateMessage, WM_USER, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::PCWSTR;

// --- GLOBAL STATE MANAGER ---
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum StandbyState {
    Normal,
    Sleep,
    Wake,
}

pub struct StateManager {
    state: Mutex<StandbyState>,
}

pub static STATE_MANAGER: once_cell::sync::Lazy<Arc<StateManager>> =
    once_cell::sync::Lazy::new(|| {
        Arc::new(StateManager {
            state: Mutex::new(StandbyState::Normal),
        })
    });

// Custom Message ID to wake up the main loop
const WM_STANDBY_CHANGE: u32 = WM_USER + 1;
static mut MAIN_HWND: HWND = HWND(null_mut());

// --- WINDOWS CALLBACKS ---

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn standby_callback(
    _: *const std::ffi::c_void,
    event_type: u32,
    _: *const std::ffi::c_void,
) -> u32 {
    unsafe {
        let mut lock = match STATE_MANAGER.state.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                warn!("Standby state mutex poisoned; recovering");
                poisoned.into_inner()
            }
        };
        let mut should_notify = false;

        match event_type {
            4 => {
                // PBT_APMSUSPEND
                *lock = StandbyState::Sleep;
                should_notify = true;
            }
            18 => {
                // PBT_APMRESUMEAUTOMATIC
                *lock = StandbyState::Wake;
                should_notify = true;
            }
            _ => {}
        }

        if should_notify {
            // Ping the main loop to handle the state change
            let _ = PostMessageW(MAIN_HWND, WM_STANDBY_CHANGE, WPARAM(0), LPARAM(0));
        }
        0
    }
}

pub struct StandbyMonitor {}

impl StandbyMonitor {
    pub fn start() {
        thread::spawn(|| {
            unsafe {
                let instance = match GetModuleHandleW(None) {
                    Ok(h) => h,
                    Err(e) => {
                        error!(error = ?e, "GetModuleHandleW failed; standby monitor will not start");
                        return;
                    }
                };
                let class_name: Vec<u16> = "RazerPowerListener\0".encode_utf16().collect();

                let wnd_class = WNDCLASSW {
                    hInstance: instance.into(),
                    lpszClassName: PCWSTR(class_name.as_ptr()),
                    lpfnWndProc: Some(wnd_proc),
                    ..Default::default()
                };
                RegisterClassW(&wnd_class);

                MAIN_HWND = match CreateWindowExW(
                    Default::default(),
                    PCWSTR(class_name.as_ptr()),
                    PCWSTR(class_name.as_ptr()),
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    instance,
                    None,
                ) {
                    Ok(hwnd) => hwnd,
                    Err(e) => {
                        error!(error = ?e, "CreateWindowExW failed; standby monitor will not start");
                        return;
                    }
                };

                let params = Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
                    Callback: Some(standby_callback),
                    Context: null_mut(),
                }));

                let _ = RegisterSuspendResumeNotification(
                    HANDLE(params as *const _ as *mut _),
                    DEVICE_NOTIFY_CALLBACK,
                );

                info!("Windows standby monitor started");

                let mut last_state = StandbyState::Wake;

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, HWND(null_mut()), 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);

                    // React only to our specific standby change signal
                    if msg.message == WM_STANDBY_CHANGE {
                        let mut lock = match STATE_MANAGER.state.lock() {
                            Ok(g) => g,
                            Err(poisoned) => {
                                warn!("Standby state mutex poisoned; recovering");
                                poisoned.into_inner()
                            }
                        };
                        if *lock != last_state {
                            match *lock {
                                StandbyState::Sleep => {
                                    razer::device_handle::device().sleep();
                                    info!(
                                        "System entering sleep; executing hardware shutdown sequence"
                                    );
                                }
                                StandbyState::Wake => {
                                    razer::device_handle::device().initialize(false);
                                    *lock = StandbyState::Normal; // Reset state
                                    info!("System waking from sleep; re-initialising hardware");
                                }
                                _ => {}
                            };
                            last_state = *lock;
                        }
                    }
                }
            };
        });
    }
}
