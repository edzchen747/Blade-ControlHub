use crate::razer;
use crate::win::system::display_gpu::GpuDisplayMonitor;

use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};
use tracing::{error, info, warn};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{
    DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY, POWERBROADCAST_SETTING,
    PowerRegisterSuspendResumeNotification, PowerUnregisterSuspendResumeNotification,
    RegisterPowerSettingNotification, RegisterSuspendResumeNotification,
    UnregisterPowerSettingNotification, UnregisterSuspendResumeNotification,
};
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::System::SystemServices::{
    GUID_CONSOLE_DISPLAY_STATE, GUID_MONITOR_POWER_ON, GUID_SESSION_DISPLAY_STATUS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DEVICE_NOTIFY_CALLBACK, DEVICE_NOTIFY_WINDOW_HANDLE, DefWindowProcW,
    DestroyWindow, DispatchMessageW, GetMessageW, MSG, PostMessageW, RegisterClassW,
    TranslateMessage, WM_POWERBROADCAST, WM_USER, WM_WTSSESSION_CHANGE, WNDCLASSW, WS_OVERLAPPED,
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
const WM_STANDBY_STOP: u32 = WM_USER + 2;
const PBT_APMSUSPEND: u32 = 4;
const PBT_APMRESUMESUSPEND: u32 = 7;
const PBT_APMRESUMEAUTOMATIC: u32 = 18;
const PBT_POWERSETTINGCHANGE: u32 = 0x8013;
const WTS_SESSION_UNLOCK_EVENT: u32 = 8;
const DISPLAY_POWER_OFF: u32 = 0;
const DISPLAY_POWER_ON: u32 = 1;
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static STANDBY_MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);
static STANDBY_MONITOR_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static STANDBY_WATCHDOG_WAKE: OnceLock<Arc<StandbyWatchdogWake>> = OnceLock::new();
static STANDBY_WATCHDOG_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static SLEEP_CLEANUP_STARTED: AtomicBool = AtomicBool::new(false);

type StandbyWatchdogWake = (Mutex<()>, Condvar);

const STANDBY_WATCHDOG_INTERVAL: Duration = Duration::from_secs(2);
const STANDBY_RESUME_GAP_THRESHOLD: Duration = Duration::from_secs(8);

// --- WINDOWS CALLBACKS ---

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst) && msg == WM_POWERBROADCAST {
        let event_type = wparam.0 as u32;
        if update_standby_state_from_event(event_type) {
            info!(event_type, "Windows standby power event received");
            if event_type == PBT_APMSUSPEND {
                // Windows waits for this window procedure during suspend. Do the
                // hardware work here rather than after DispatchMessageW returns.
                run_sleep_cleanup("suspend_notification");
            }
        }

        if event_type == PBT_POWERSETTINGCHANGE
            && unsafe { update_standby_state_from_power_setting_lparam(lparam) }
        {
            // On Modern Standby, display-off is the earlier pre-sleep signal.
            // Running while Windows dispatches it keeps the HID sequence within
            // the screen-off preparation window.
            run_sleep_cleanup("display_power_off");
        }

        return LRESULT(1);
    }

    if STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst)
        && msg == WM_WTSSESSION_CHANGE
        && session_change_is_wake(wparam.0 as u32)
        && update_standby_state_from_session_resume()
    {
        info!(event_type = wparam.0, "Windows session wake event received");
    }

    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn standby_callback(
    _: *const std::ffi::c_void,
    event_type: u32,
    _: *const std::ffi::c_void,
) -> u32 {
    if STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst) && update_standby_state_from_event(event_type)
    {
        info!(event_type, "Windows standby callback event received");
        wake_standby_monitor(WM_STANDBY_CHANGE);
    }
    0
}

