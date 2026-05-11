use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExA, DefWindowProcA, DispatchMessageA, GetMessageA, PBT_APMPOWERSTATUSCHANGE,
    RegisterClassA, WM_POWERBROADCAST, WNDCLASSA,
};

use crate::razer::device_handle::device;

pub static IS_PLUGGED_IN: AtomicBool = AtomicBool::new(false);

pub struct PowerMonitor {}

impl PowerMonitor {
    pub fn start() {
        // Initial sync so the state is correct before the first event
        sync_power_state();

        thread::spawn(move || {
            unsafe {
                let window_class = "PowerEventWindow\0".as_ptr();
                let wnd_class = WNDCLASSA {
                    lpfnWndProc: Some(power_wnd_proc),
                    hInstance: 0,
                    lpszClassName: window_class,
                    ..std::mem::zeroed()
                };

                RegisterClassA(&wnd_class);

                // "Message-Only" window (invisible, no UI)
                let hwnd = CreateWindowExA(
                    0,
                    window_class,
                    "\0".as_ptr(),
                    0, // No WS_VISIBLE flag means it stays hidden
                    0,
                    0,
                    0,
                    0,
                    0, // Change from -3 to 0
                    0,
                    0,
                    std::ptr::null(),
                );

                if hwnd == 0 {
                    return;
                }

                // GetMessageA blocks the thread until an event occurs
                let mut msg = std::mem::zeroed();
                while GetMessageA(&mut msg, 0, 0, 0) > 0 {
                    DispatchMessageA(&msg);
                }
            }
        });
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
        // The OS has sent a broadcast (plugged in, battery low, etc.)
        sync_power_state();
    }
    // Let Windows handle the rest of the window overhead
    unsafe { DefWindowProcA(hwnd, msg, wparam, lparam) }
}

fn sync_power_state() {
    unsafe {
        let mut status: SYSTEM_POWER_STATUS = std::mem::zeroed();
        if GetSystemPowerStatus(&mut status) != 0 {
            let plugged_in = status.ACLineStatus == 1;
            let last_state = { IS_PLUGGED_IN.load(Ordering::SeqCst) };
            if last_state != plugged_in {
                IS_PLUGGED_IN.store(plugged_in, Ordering::SeqCst);
                thread::spawn(move || {
                    // Strange Windows behaviour - it will re-set brightness on unplug / plug,
                    // so we delay to ensure our brightness action happens last
                    thread::sleep(Duration::from_millis(500));
                    device().initialize();
                });
            }
            println!(
                "Power Event received: {}",
                if plugged_in { "AC" } else { "Battery" }
            );
        }
    }
}
