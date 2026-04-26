use std::sync::{Arc, Mutex, Condvar};
use std::thread;
use std::ptr::null_mut;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, HANDLE, WPARAM, LPARAM, LRESULT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    TranslateMessage, MSG, WNDCLASSW, WS_OVERLAPPED, DEVICE_NOTIFY_CALLBACK,
};
use windows::Win32::System::Power::{
    RegisterSuspendResumeNotification, DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS,
};

/// 0: Normal, 1: Sleep, 2: Wake
#[derive(Debug, PartialEq, Clone, Copy)]
enum PowerState {
    Normal,
    Sleep,
    Wake,
}

struct StateManager {
    state: Mutex<PowerState>,
    cvar: Condvar,
}

/// Global state manager to communicate between Windows Callback and Main Loop
static STATE_MANAGER: once_cell::sync::Lazy<Arc<StateManager>> = once_cell::sync::Lazy::new(|| {
    Arc::new(StateManager {
        state: Mutex::new(PowerState::Normal),
        cvar: Condvar::new(),
    })
});

/// Window Procedure Proxy: Crucial for the "system" calling convention mismatch
unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// The callback Windows executes when power state changes
unsafe extern "system" fn power_callback(
    _context: *const std::ffi::c_void,
    event_type: u32,
    _setting: *const std::ffi::c_void,
) -> u32 {
    let mut lock = STATE_MANAGER.state.lock().unwrap();
    match event_type {
        4 => { // PBT_APMSUSPEND
            *lock = PowerState::Sleep;
            STATE_MANAGER.cvar.notify_all();
        }
        18 => { // PBT_APMRESUMEAUTOMATIC
            *lock = PowerState::Wake;
            STATE_MANAGER.cvar.notify_all();
        }
        _ => {}
    }
    0
}

fn spawn_listener_thread() {
    thread::spawn(|| unsafe {
        let instance = GetModuleHandleW(None).expect("Failed to get module handle");
        let class_name: Vec<u16> = "RazerPowerListener\0".encode_utf16().collect();
        
        let wnd_class = WNDCLASSW {
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            lpfnWndProc: Some(wnd_proc), // Points to our extern "system" proxy
            ..Default::default()
        };

        RegisterClassW(&wnd_class);

        // Create a Message-Only Window (invisible) to catch broadcast messages
        let _hwnd = CreateWindowExW(
            Default::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(class_name.as_ptr()),
            WS_OVERLAPPED,
            0, 0, 0, 0,
            None, None,
            instance,
            None,
        ).expect("Failed to create message window");

        // Prepare the notification parameters (must leak to remain valid)
        let params = Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(power_callback),
            Context: null_mut(),
        }));

        // Register for Suspend/Resume events
        let _ = RegisterSuspendResumeNotification(
            HANDLE(params as *const _ as *mut _),
            DEVICE_NOTIFY_CALLBACK,
        );

        // THE MESSAGE PUMP
        // Keeps this thread alive and responsive to Windows messages
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND(null_mut()), 0, 0).as_bool() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

fn main() -> anyhow::Result<()> {
    println!("--- Razer Power Monitor Starting ---");
    
    spawn_listener_thread();

    // Secondary thread for your 10ms hardware logic (e.g. Fn key HID polling)
    thread::spawn(|| {
        loop {
            // Only perform HID actions if the system isn't currently suspended
            let state = *STATE_MANAGER.state.lock().unwrap();
            if state == PowerState::Normal {
                // TODO: Insert your Razer/HID polling logic here
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    // Main Control Loop: High-priority response to Power Events
    loop {
        let mut lock = STATE_MANAGER.state.lock().unwrap();
        
        // Park the thread (0% CPU) until the state is something other than Normal
        lock = STATE_MANAGER.cvar.wait_while(lock, |s| *s == PowerState::Normal).unwrap();

        match *lock {
            PowerState::Sleep => {
                println!("[!] Event: System going to Sleep. Shutting down Razer effects...");
                // razer_hid::close_handles();
                *lock = PowerState::Normal; 
            }
            PowerState::Wake => {
                println!("[+] Event: System Woke Up. Re-initializing hardware...");
                // razer_hid::reconnect();
                *lock = PowerState::Normal;
            }
            _ => {}
        }
    }
}