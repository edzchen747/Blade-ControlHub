use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExA, DefWindowProcA, DispatchMessageA, GetMessageA, PBT_APMPOWERSTATUSCHANGE,
    RegisterClassA, WM_POWERBROADCAST, WNDCLASSA,
};

pub static IS_PLUGGED_IN: AtomicBool = AtomicBool::new(false);

pub struct PowerMonitor {
    _thread_handle: thread::JoinHandle<()>,
}

impl PowerMonitor {
    pub fn new() -> Self {
        // Initial sync so the state is correct before the first event
        Self::sync_power_state();

        let handle = thread::spawn(move || {
            unsafe {
                let window_class = "PowerEventWindow\0".as_ptr();
                let wnd_class = WNDCLASSA {
                    lpfnWndProc: Some(power_wnd_proc),
                    hInstance: 0,
                    lpszClassName: window_class,
                    ..std::mem::zeroed()
                };

                RegisterClassA(&wnd_class);

                // Create a "Message-Only" window (invisible, no UI)
                let hwnd = CreateWindowExA(
                    0,
                    window_class,
                    "\0".as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    -3, // HWND_MESSAGE: Makes it a message-only window
                    0,
                    0,
                    std::ptr::null(),
                );

                if hwnd == 0 {
                    return;
                }

                // The Blocking Loop: GetMessageA blocks the thread until an event occurs
                let mut msg = std::mem::zeroed();
                while GetMessageA(&mut msg, hwnd, 0, 0) > 0 {
                    DispatchMessageA(&msg);
                }
            }
        });

        Self {
            _thread_handle: handle,
        }
    }

    fn sync_power_state() {
        unsafe {
            let mut status: SYSTEM_POWER_STATUS = std::mem::zeroed();
            if GetSystemPowerStatus(&mut status) != 0 {
                IS_PLUGGED_IN.store(status.ACLineStatus == 1, Ordering::SeqCst);
            }
        }
    }
}

/// The callback Windows triggers when the invisible window receives a message
unsafe extern "system" fn power_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_POWERBROADCAST && wparam as u32 == PBT_APMPOWERSTATUSCHANGE {
        // The OS is telling us something changed (plugged in, battery low, etc.)
        let mut status: SYSTEM_POWER_STATUS = std::mem::zeroed();
        if GetSystemPowerStatus(&mut status) != 0 {
            let plugged_in = status.ACLineStatus == 1;
            IS_PLUGGED_IN.store(plugged_in, Ordering::SeqCst);
            println!(
                "Event received: Power is now {}",
                if plugged_in { "AC" } else { "Battery" }
            );
        }
    }
    // Let Windows handle the rest of the window overhead
    DefWindowProcA(hwnd, msg, wparam, lparam)
}
