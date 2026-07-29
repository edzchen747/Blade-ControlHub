use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::{info, warn};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExA, DefWindowProcA, DestroyWindow, DispatchMessageA, GetMessageA, MSG,
    PBT_APMPOWERSTATUSCHANGE, PostThreadMessageA, RegisterClassA, WM_POWERBROADCAST, WM_QUIT,
    WNDCLASSA,
};

use crate::core::shared_state::IS_PLUGGED_IN;
use crate::razer::device_handle::device;

pub struct PowerMonitor {}

static POWER_MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);
static POWER_MONITOR_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static POWER_MONITOR_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

impl PowerMonitor {
    pub fn start() {
        join_finished_power_monitor_thread();

        if POWER_MONITOR_RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }

        // Initial sync so the state is correct before the first event
        sync_power_state();

        match thread::Builder::new()
            .name("blade-power-monitor".to_string())
            .spawn(run_power_message_loop)
        {
            Ok(handle) => {
                *power_monitor_thread() = Some(handle);
            }
            Err(error) => {
                POWER_MONITOR_RUNNING.store(false, Ordering::SeqCst);
                warn!(%error, "Failed to start power monitor thread");
            }
        }
    }

    pub fn stop() {
        POWER_MONITOR_RUNNING.store(false, Ordering::SeqCst);
        wake_power_monitor();
        join_power_monitor_thread();
    }
}

fn power_monitor_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    POWER_MONITOR_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_power_monitor_thread() {
    let should_join = power_monitor_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);

    if should_join {
        join_power_monitor_thread();
    }
}

fn join_power_monitor_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = power_monitor_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current power monitor thread during shutdown");
        *power_monitor_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("Power monitor thread panicked during shutdown");
    }
}

fn run_power_message_loop() {
    POWER_MONITOR_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);

    const WINDOW_CLASS: &[u8] = b"PowerEventWindow\0";
    const WINDOW_TITLE: &[u8] = b"\0";

    let Some(_window) = PowerWindow::create(WINDOW_CLASS, WINDOW_TITLE) else {
        warn!("Power monitor window could not be created");
        POWER_MONITOR_RUNNING.store(false, Ordering::SeqCst);
        POWER_MONITOR_THREAD_ID.store(0, Ordering::SeqCst);
        return;
    };

    run_power_window_loop();
    POWER_MONITOR_THREAD_ID.store(0, Ordering::SeqCst);
    POWER_MONITOR_RUNNING.store(false, Ordering::SeqCst);
}

fn run_power_window_loop() {
    while POWER_MONITOR_RUNNING.load(Ordering::SeqCst) {
        let mut msg = MaybeUninit::<MSG>::uninit();
        let result = unsafe { GetMessageA(msg.as_mut_ptr(), 0, 0, 0) };

        match result {
            value if value > 0 => {
                // SAFETY: GetMessageA returned > 0, so it initialized `msg`.
                let msg = unsafe { msg.assume_init() };
                unsafe { DispatchMessageA(&msg) };
            }
            0 => break,
            _ => {
                warn!("Power monitor message loop failed");
                break;
            }
        }
    }
}

fn wake_power_monitor() {
    let thread_id = POWER_MONITOR_THREAD_ID.load(Ordering::SeqCst);
    if thread_id != 0 {
        let _ = unsafe { PostThreadMessageA(thread_id, WM_QUIT, 0, 0) };
    }
}

struct PowerWindow {
    hwnd: HWND,
}

impl PowerWindow {
    fn create(window_class: &[u8], window_title: &[u8]) -> Option<Self> {
        let hwnd = create_power_window(window_class, window_title);
        (hwnd != 0).then_some(Self { hwnd })
    }
}

impl Drop for PowerWindow {
    fn drop(&mut self) {
        if self.hwnd != 0 {
            // Best-effort cleanup for the hidden message-only style window when
            // the message loop exits during process shutdown.
            let _ = unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

fn create_power_window(window_class: &[u8], window_title: &[u8]) -> HWND {
    unsafe {
        let wnd_class = WNDCLASSA {
            lpfnWndProc: Some(power_wnd_proc),
            hInstance: 0,
            lpszClassName: window_class.as_ptr(),
            ..zeroed_wnd_class()
        };

        RegisterClassA(&wnd_class);

        CreateWindowExA(
            0,
            window_class.as_ptr(),
            window_title.as_ptr(),
            0, // No WS_VISIBLE flag means it stays hidden
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            std::ptr::null(),
        )
    }
}

/// The callback Windows triggers when the invisible window receives a message
unsafe extern "system" fn power_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if POWER_MONITOR_RUNNING.load(Ordering::SeqCst)
        && msg == WM_POWERBROADCAST
        && wparam as u32 == PBT_APMPOWERSTATUSCHANGE
    {
        // The OS has sent a broadcast (plugged in, battery low, etc.)
        sync_power_state();
    }
    // Let Windows handle the rest of the window overhead
    unsafe { DefWindowProcA(hwnd, msg, wparam, lparam) }
}

fn sync_power_state() {
    let Some(status) = get_system_power_status() else {
        warn!("Failed to read system power status");
        return;
    };

    let plugged_in = is_ac_connected(&status);
    let last_state = IS_PLUGGED_IN.load(Ordering::SeqCst);
    if last_state != plugged_in {
        IS_PLUGGED_IN.store(plugged_in, Ordering::SeqCst);
        if let Err(error) = thread::Builder::new()
            .name("blade-power-reinitialize".to_string())
            .spawn(move || {
                // Windows resets brightness shortly after AC changes, so delay
                // to ensure our hardware sync happens last.
                thread::sleep(Duration::from_millis(500));
                if POWER_MONITOR_RUNNING.load(Ordering::SeqCst) {
                    device().initialize(false);
                }
            })
        {
            warn!(%error, "Failed to start power reinitialization worker");
        }
    }
    info!(plugged_in, "Power state changed");
}

fn get_system_power_status() -> Option<SYSTEM_POWER_STATUS> {
    let mut status = zeroed_power_status();
    (unsafe { GetSystemPowerStatus(&mut status) } != 0).then_some(status)
}

fn is_ac_connected(status: &SYSTEM_POWER_STATUS) -> bool {
    status.ACLineStatus == 1
}

fn zeroed_wnd_class() -> WNDCLASSA {
    // SAFETY: WNDCLASSA is a plain C struct. Zero is the documented default
    // for fields we do not populate before RegisterClassA.
    unsafe { std::mem::zeroed() }
}

fn zeroed_power_status() -> SYSTEM_POWER_STATUS {
    // SAFETY: SYSTEM_POWER_STATUS is a plain C output struct populated by
    // GetSystemPowerStatus before any production code reads its fields.
    unsafe { std::mem::zeroed() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status_with_ac_line(ac_line_status: u8) -> SYSTEM_POWER_STATUS {
        SYSTEM_POWER_STATUS {
            ACLineStatus: ac_line_status,
            BatteryFlag: 0,
            BatteryLifePercent: 0,
            SystemStatusFlag: 0,
            BatteryLifeTime: 0,
            BatteryFullLifeTime: 0,
        }
    }

    #[test]
    fn ac_line_status_one_maps_to_plugged_in() {
        assert!(is_ac_connected(&status_with_ac_line(1)));
    }

    #[test]
    fn ac_line_status_zero_maps_to_battery() {
        assert!(!is_ac_connected(&status_with_ac_line(0)));
    }

    #[test]
    fn ac_line_status_unknown_maps_to_not_plugged_in() {
        assert!(!is_ac_connected(&status_with_ac_line(255)));
    }

    #[test]
    fn stop_clears_power_monitor_running_flag() {
        POWER_MONITOR_RUNNING.store(true, Ordering::SeqCst);

        PowerMonitor::stop();

        assert!(!POWER_MONITOR_RUNNING.load(Ordering::SeqCst));
    }

    #[test]
    fn join_power_monitor_thread_drains_handle() {
        *power_monitor_thread() = Some(thread::spawn(|| {}));

        join_power_monitor_thread();

        assert!(power_monitor_thread().is_none());
    }
}
