use crate::razer;

use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use tracing::{error, info, warn};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{
    DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY, RegisterSuspendResumeNotification,
    UnregisterSuspendResumeNotification,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DEVICE_NOTIFY_CALLBACK, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, MSG, PostMessageW, RegisterClassW, TranslateMessage, WM_USER, WNDCLASSW,
    WS_OVERLAPPED,
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
const PBT_APMRESUMEAUTOMATIC: u32 = 18;
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static STANDBY_MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);
static STANDBY_MONITOR_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

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
    if STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst) && update_standby_state_from_event(event_type)
    {
        let hwnd = HWND(MAIN_HWND.load(Ordering::SeqCst) as *mut _);
        if !hwnd.0.is_null() {
            // Ping the main loop to handle the state change
            let _ = unsafe { PostMessageW(hwnd, WM_STANDBY_CHANGE, WPARAM(0), LPARAM(0)) };
        }
    }
    0
}

pub struct StandbyMonitor {}

impl StandbyMonitor {
    pub fn start() {
        join_finished_standby_monitor_thread();

        if STANDBY_MONITOR_RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }

        match thread::Builder::new()
            .name("blade-standby-monitor".to_string())
            .spawn(run_standby_monitor_loop)
        {
            Ok(handle) => {
                *standby_monitor_thread() = Some(handle);
            }
            Err(error) => {
                STANDBY_MONITOR_RUNNING.store(false, Ordering::SeqCst);
                error!(%error, "Failed to start standby monitor thread");
            }
        }
    }

    pub fn stop() {
        STANDBY_MONITOR_RUNNING.store(false, Ordering::SeqCst);
        wake_standby_monitor(WM_STANDBY_STOP);
        join_standby_monitor_thread();
    }
}

fn standby_monitor_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    STANDBY_MONITOR_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_standby_monitor_thread() {
    let should_join = standby_monitor_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);

    if should_join {
        join_standby_monitor_thread();
    }
}

fn join_standby_monitor_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = standby_monitor_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current standby monitor thread during shutdown");
        *standby_monitor_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("Standby monitor thread panicked during shutdown");
    }
}

fn run_standby_monitor_loop() {
    let Some(window) = StandbyWindow::create() else {
        STANDBY_MONITOR_RUNNING.store(false, Ordering::SeqCst);
        return;
    };
    let hwnd = window.hwnd;
    MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

    let Some(_subscription) = StandbySubscription::register() else {
        error!("RegisterSuspendResumeNotification failed; standby monitor will not start");
        MAIN_HWND.store(0, Ordering::SeqCst);
        STANDBY_MONITOR_RUNNING.store(false, Ordering::SeqCst);
        return;
    };

    info!("Windows standby monitor started");

    let mut last_state = StandbyState::Wake;
    run_message_loop(&mut last_state);
    MAIN_HWND.store(0, Ordering::SeqCst);
    STANDBY_MONITOR_RUNNING.store(false, Ordering::SeqCst);
}

fn wake_standby_monitor(message: u32) {
    let hwnd = HWND(MAIN_HWND.load(Ordering::SeqCst) as *mut _);
    if !hwnd.0.is_null() {
        let _ = unsafe { PostMessageW(hwnd, message, WPARAM(0), LPARAM(0)) };
    }
}

fn run_message_loop(last_state: &mut StandbyState) {
    unsafe {
        let mut msg = MSG::default();
        while STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst)
            && GetMessageW(&mut msg, HWND(null_mut()), 0, 0).as_bool()
        {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);

            match msg.message {
                WM_STANDBY_CHANGE => process_standby_change(last_state),
                WM_STANDBY_STOP => break,
                _ => {}
            }
        }
    }
}

fn process_standby_change(last_state: &mut StandbyState) {
    if !STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst) {
        return;
    }

    let mut lock = standby_state();
    if *lock == *last_state {
        return;
    }

    match *lock {
        StandbyState::Sleep => {
            let _ = razer::device_handle::device().sleep();
            info!("System entering sleep; executing hardware shutdown sequence");
        }
        StandbyState::Wake => {
            razer::device_handle::device().initialize(false);
            *lock = StandbyState::Normal;
            info!("System waking from sleep; re-initialising hardware");
        }
        StandbyState::Normal => {}
    };
    *last_state = *lock;
}

struct StandbyWindow {
    hwnd: HWND,
}

impl StandbyWindow {
    fn create() -> Option<Self> {
        create_standby_window().map(|hwnd| Self { hwnd })
    }
}

impl Drop for StandbyWindow {
    fn drop(&mut self) {
        if self.hwnd.0.is_null() {
            return;
        }

        if let Err(error) = unsafe { DestroyWindow(self.hwnd) } {
            warn!(?error, "Failed to destroy standby monitor window");
        }
    }
}

fn create_standby_window() -> Option<HWND> {
    unsafe {
        let instance = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(e) => {
                error!(error = ?e, "GetModuleHandleW failed; standby monitor will not start");
                return None;
            }
        };
        let class_name: Vec<u16> = "RazerPowerListener\0".encode_utf16().collect();

        let wnd_class = WNDCLASSW {
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            lpfnWndProc: Some(wnd_proc),
            ..Default::default()
        };
        let _ = RegisterClassW(&wnd_class);

        match CreateWindowExW(
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
            Ok(hwnd) => Some(hwnd),
            Err(e) => {
                error!(error = ?e, "CreateWindowExW failed; standby monitor will not start");
                None
            }
        }
    }
}

fn standby_state() -> std::sync::MutexGuard<'static, StandbyState> {
    match STATE_MANAGER.state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("Standby state mutex poisoned; recovering");
            poisoned.into_inner()
        }
    }
}

fn update_standby_state_from_event(event_type: u32) -> bool {
    let mut lock = standby_state();
    match event_type {
        PBT_APMSUSPEND => {
            *lock = StandbyState::Sleep;
            true
        }
        PBT_APMRESUMEAUTOMATIC => {
            *lock = StandbyState::Wake;
            true
        }
        _ => false,
    }
}

struct StandbySubscription {
    handle: HPOWERNOTIFY,
    _params: Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>,
}

impl StandbySubscription {
    fn register() -> Option<Self> {
        let mut params = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(standby_callback),
            Context: null_mut(),
        });
        let handle = unsafe {
            RegisterSuspendResumeNotification(
                HANDLE((&mut *params) as *mut _ as *mut _),
                DEVICE_NOTIFY_CALLBACK,
            )
        }
        .ok()?;

        Some(Self {
            handle,
            _params: params,
        })
    }
}

impl Drop for StandbySubscription {
    fn drop(&mut self) {
        if let Err(error) = unsafe { UnregisterSuspendResumeNotification(self.handle) } {
            warn!(?error, "Failed to unregister standby notification");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn standby_event_mapping_updates_state_for_sleep_and_wake() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        assert!(update_standby_state_from_event(PBT_APMSUSPEND));
        assert_eq!(*standby_state(), StandbyState::Sleep);

        assert!(update_standby_state_from_event(PBT_APMRESUMEAUTOMATIC));
        assert_eq!(*standby_state(), StandbyState::Wake);
    }

    #[test]
    fn standby_event_mapping_ignores_unknown_event() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        *standby_state() = StandbyState::Normal;

        assert!(!update_standby_state_from_event(u32::MAX));
        assert_eq!(*standby_state(), StandbyState::Normal);
    }

    #[test]
    fn stop_clears_standby_monitor_running_flag() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        STANDBY_MONITOR_RUNNING.store(true, Ordering::SeqCst);
        MAIN_HWND.store(0, Ordering::SeqCst);

        StandbyMonitor::stop();

        assert!(!STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst));
        assert_eq!(MAIN_HWND.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn join_standby_monitor_thread_drains_handle() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *standby_monitor_thread() = Some(thread::spawn(|| {}));

        join_standby_monitor_thread();

        assert!(standby_monitor_thread().is_none());
    }
}
