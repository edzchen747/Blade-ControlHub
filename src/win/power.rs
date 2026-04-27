use crate::razer;

use std::sync::{Arc, Mutex};
use std::ptr::null_mut;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, HANDLE, WPARAM, LPARAM, LRESULT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    TranslateMessage, MSG, WNDCLASSW, WS_OVERLAPPED, DEVICE_NOTIFY_CALLBACK, 
    PostMessageW, WM_USER,
};
use windows::Win32::System::Power::{
    RegisterSuspendResumeNotification, DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS,
};

// --- GLOBAL STATE MANAGER ---
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PowerState { Normal, Sleep, Wake }

pub struct StateManager {
    state: Mutex<PowerState>,
}

pub static STATE_MANAGER: once_cell::sync::Lazy<Arc<StateManager>> = once_cell::sync::Lazy::new(|| {
    Arc::new(StateManager { state: Mutex::new(PowerState::Normal) })
});

// Custom Message ID to wake up the main loop
const WM_POWER_CHANGE: u32 = WM_USER + 1;
static mut MAIN_HWND: HWND = HWND(null_mut());

// --- WINDOWS CALLBACKS ---

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn power_callback(_: *const std::ffi::c_void, event_type: u32, _: *const std::ffi::c_void) -> u32 {
    let mut lock = STATE_MANAGER.state.lock().unwrap();
    let mut should_notify = false;

    match event_type {
        4 => { // PBT_APMSUSPEND
            *lock = PowerState::Sleep;
            should_notify = true;
        }
        18 => { // PBT_APMRESUMEAUTOMATIC
            *lock = PowerState::Wake;
            should_notify = true;
        }
        _ => {}
    }

    if should_notify {
        // Ping the main loop to handle the state change
        let _ = PostMessageW(MAIN_HWND, WM_POWER_CHANGE, WPARAM(0), LPARAM(0));
    }
    0
}

pub fn spawn_listener_thread() -> anyhow::Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name: Vec<u16> = "RazerPowerListener\0".encode_utf16().collect();
        
        let wnd_class = WNDCLASSW {
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            lpfnWndProc: Some(wnd_proc),
            ..Default::default()
        };
        RegisterClassW(&wnd_class);

        MAIN_HWND = CreateWindowExW(
            Default::default(), PCWSTR(class_name.as_ptr()), PCWSTR(class_name.as_ptr()),
            WS_OVERLAPPED, 0, 0, 0, 0, None, None, instance, None,
        )?;

        let params = Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(power_callback),
            Context: null_mut(),
        }));
        
        let _ = RegisterSuspendResumeNotification(HANDLE(params as *const _ as *mut _), DEVICE_NOTIFY_CALLBACK);

        println!("\n--- Windows Power Monitor Started ---");

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND(null_mut()), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);

            // React only to our specific power change signal
            if msg.message == WM_POWER_CHANGE {
                let mut lock = STATE_MANAGER.state.lock().unwrap();
                match *lock {
                    PowerState::Sleep => {
                        println!("[!] State updated to SLEEP. Handling hardware shutdown...");
                        let _ = razer::device_handle::device().keyboard_sleep();
                    }
                    PowerState::Wake => {
                        println!("[+] State updated to WAKE. Handling hardware re-init...");
                        let _ = razer::device_handle::device().initialize_keyboard();
                        *lock = PowerState::Normal; // Reset state
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}