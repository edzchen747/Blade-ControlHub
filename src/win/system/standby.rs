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
    if STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst)
        && msg == WM_POWERBROADCAST
        && update_standby_state_from_event(wparam.0 as u32)
    {
        info!(
            event_type = wparam.0,
            "Windows standby power event received"
        );
    }

    if STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst)
        && msg == WM_POWERBROADCAST
        && wparam.0 as u32 == PBT_POWERSETTINGCHANGE
    {
        unsafe { update_standby_state_from_power_setting_lparam(lparam) };
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

pub struct StandbyMonitor {}

impl StandbyMonitor {
    pub fn start() {
        join_finished_standby_monitor_thread();
        join_finished_standby_watchdog_thread();

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
        standby_watchdog_wake().1.notify_all();
        wake_standby_monitor(WM_STANDBY_STOP);
        join_standby_monitor_thread();
        join_standby_watchdog_thread();
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

fn standby_watchdog_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    STANDBY_WATCHDOG_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_standby_watchdog_thread() {
    let should_join = standby_watchdog_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);

    if should_join {
        join_standby_watchdog_thread();
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

fn join_standby_watchdog_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = standby_watchdog_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current standby watchdog thread during shutdown");
        *standby_watchdog_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("Standby watchdog thread panicked during shutdown");
    }
}

fn run_standby_monitor_loop() {
    let Some(window) = StandbyWindow::create() else {
        STANDBY_MONITOR_RUNNING.store(false, Ordering::SeqCst);
        return;
    };
    let hwnd = window.hwnd;
    MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

    let _subscription = StandbySubscriptions::register(hwnd);

    info!("Windows standby monitor started");
    start_standby_watchdog();

    let mut last_state = StandbyState::Normal;
    run_message_loop(&mut last_state);
    MAIN_HWND.store(0, Ordering::SeqCst);
    STANDBY_MONITOR_RUNNING.store(false, Ordering::SeqCst);
    standby_watchdog_wake().1.notify_all();
    join_standby_watchdog_thread();
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
            && GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool()
        {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);

            match msg.message {
                WM_POWERBROADCAST => process_standby_change(last_state),
                WM_WTSSESSION_CHANGE => process_standby_change(last_state),
                WM_STANDBY_CHANGE => process_standby_change(last_state),
                WM_STANDBY_STOP => break,
                _ => {}
            }
        }
    }
}

fn start_standby_watchdog() {
    join_finished_standby_watchdog_thread();

    if standby_watchdog_thread().is_some() {
        warn!("Standby watchdog is already running");
        return;
    }

    match thread::Builder::new()
        .name("blade-standby-watchdog".to_string())
        .spawn(run_standby_watchdog_loop)
    {
        Ok(handle) => {
            *standby_watchdog_thread() = Some(handle);
        }
        Err(error) => {
            warn!(%error, "Failed to start standby watchdog thread");
        }
    }
}

fn run_standby_watchdog_loop() {
    let mut last_tick = SystemTime::now();
    while STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst) {
        wait_for_standby_watchdog(STANDBY_WATCHDOG_INTERVAL);
        if !STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst) {
            break;
        }

        let now = SystemTime::now();
        if let Ok(elapsed) = now.duration_since(last_tick)
            && resume_gap_detected(elapsed)
        {
            info!(
                elapsed_ms = elapsed.as_millis() as u64,
                "Standby watchdog detected resume after system sleep"
            );
            if update_standby_state_from_watchdog_resume() {
                wake_standby_monitor(WM_STANDBY_CHANGE);
            }
        }
        last_tick = now;
    }
}

fn standby_watchdog_wake() -> Arc<StandbyWatchdogWake> {
    STANDBY_WATCHDOG_WAKE
        .get_or_init(|| Arc::new((Mutex::new(()), Condvar::new())))
        .clone()
}

fn wait_for_standby_watchdog(duration: Duration) {
    let signal = standby_watchdog_wake();
    let (lock, cvar) = &*signal;
    let guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (_guard, _timeout) = cvar
        .wait_timeout_while(guard, duration, |_| {
            STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst)
        })
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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
            recover_from_system_wake("power_event", true);
            *lock = StandbyState::Normal;
        }
        StandbyState::Normal => {}
    };
    *last_state = *lock;
}

fn recover_from_system_wake(source: &'static str, refresh_display: bool) {
    razer::device_handle::device().initialize(false);
    if refresh_display {
        GpuDisplayMonitor::trigger_display_change();
    }
    info!(source, "System waking from sleep; re-initialising hardware");
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
        PBT_APMRESUMESUSPEND => {
            *lock = StandbyState::Wake;
            true
        }
        _ => false,
    }
}

fn update_standby_state_from_watchdog_resume() -> bool {
    let mut lock = standby_state();
    if *lock == StandbyState::Wake {
        return false;
    }

    *lock = StandbyState::Wake;
    true
}

fn update_standby_state_from_session_resume() -> bool {
    let mut lock = standby_state();
    if *lock == StandbyState::Wake {
        return false;
    }

    *lock = StandbyState::Wake;
    true
}

unsafe fn update_standby_state_from_power_setting_lparam(lparam: LPARAM) -> bool {
    let setting = lparam.0 as *const POWERBROADCAST_SETTING;
    if setting.is_null() {
        return false;
    }

    let setting = unsafe { &*setting };
    let Some(value) = power_setting_data_u32(setting) else {
        return false;
    };
    let Some((name, state)) = display_power_setting_state(setting.PowerSetting, value) else {
        return false;
    };

    info!(
        setting = name,
        value,
        state = ?state,
        "Windows display power setting event received"
    );
    update_standby_state_from_display_power(state)
}

fn power_setting_data_u32(setting: &POWERBROADCAST_SETTING) -> Option<u32> {
    if setting.DataLength < std::mem::size_of::<u32>() as u32 {
        return None;
    }

    Some(unsafe { std::ptr::read_unaligned(setting.Data.as_ptr() as *const u32) })
}

fn display_power_setting_state(
    guid: windows::core::GUID,
    value: u32,
) -> Option<(&'static str, StandbyState)> {
    let name = display_power_setting_name(guid)?;
    match value {
        DISPLAY_POWER_OFF => Some((name, StandbyState::Sleep)),
        DISPLAY_POWER_ON => Some((name, StandbyState::Wake)),
        _ => None,
    }
}

fn display_power_setting_name(guid: windows::core::GUID) -> Option<&'static str> {
    if guid == GUID_CONSOLE_DISPLAY_STATE {
        Some("console_display_state")
    } else if guid == GUID_SESSION_DISPLAY_STATUS {
        Some("session_display_status")
    } else if guid == GUID_MONITOR_POWER_ON {
        Some("monitor_power_on")
    } else {
        None
    }
}

fn update_standby_state_from_display_power(state: StandbyState) -> bool {
    let mut lock = standby_state();
    if *lock == state {
        return false;
    }

    *lock = state;
    true
}

fn session_change_is_wake(event_type: u32) -> bool {
    event_type == WTS_SESSION_UNLOCK_EVENT
}

fn resume_gap_detected(elapsed: Duration) -> bool {
    elapsed >= STANDBY_RESUME_GAP_THRESHOLD
}

struct WindowStandbySubscription {
    handle: HPOWERNOTIFY,
}

impl WindowStandbySubscription {
    fn register(hwnd: HWND) -> Option<Self> {
        let handle = unsafe {
            RegisterSuspendResumeNotification(HANDLE(hwnd.0 as *mut _), DEVICE_NOTIFY_WINDOW_HANDLE)
        }
        .ok()?;

        Some(Self { handle })
    }
}

impl Drop for WindowStandbySubscription {
    fn drop(&mut self) {
        if let Err(error) = unsafe { UnregisterSuspendResumeNotification(self.handle) } {
            warn!(?error, "Failed to unregister standby window notification");
        }
    }
}

struct SessionStandbySubscription {
    hwnd: HWND,
}

impl SessionStandbySubscription {
    fn register(hwnd: HWND) -> Option<Self> {
        unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) }
            .ok()
            .map(|_| Self { hwnd })
    }
}

impl Drop for SessionStandbySubscription {
    fn drop(&mut self) {
        if let Err(error) = unsafe { WTSUnRegisterSessionNotification(self.hwnd) } {
            warn!(?error, "Failed to unregister standby session notification");
        }
    }
}

struct PowerSettingStandbySubscription {
    registrations: Vec<PowerSettingRegistration>,
}

impl PowerSettingStandbySubscription {
    fn register(hwnd: HWND) -> Option<Self> {
        let registrations = [
            ("console_display_state", GUID_CONSOLE_DISPLAY_STATE),
            ("session_display_status", GUID_SESSION_DISPLAY_STATUS),
            ("monitor_power_on", GUID_MONITOR_POWER_ON),
        ]
        .into_iter()
        .filter_map(|(name, guid)| PowerSettingRegistration::register(hwnd, name, &guid))
        .collect::<Vec<_>>();

        (!registrations.is_empty()).then_some(Self { registrations })
    }
}

impl Drop for PowerSettingStandbySubscription {
    fn drop(&mut self) {
        self.registrations.clear();
    }
}

struct PowerSettingRegistration {
    name: &'static str,
    handle: HPOWERNOTIFY,
}

impl PowerSettingRegistration {
    fn register(hwnd: HWND, name: &'static str, guid: &windows::core::GUID) -> Option<Self> {
        let handle = unsafe {
            RegisterPowerSettingNotification(
                HANDLE(hwnd.0 as *mut _),
                guid,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            )
        };

        match handle {
            Ok(handle) => {
                info!(
                    setting = name,
                    "Registered standby power setting notification"
                );
                Some(Self { name, handle })
            }
            Err(error) => {
                warn!(
                    ?error,
                    setting = name,
                    "Standby power setting registration failed"
                );
                None
            }
        }
    }
}

impl Drop for PowerSettingRegistration {
    fn drop(&mut self) {
        if let Err(error) = unsafe { UnregisterPowerSettingNotification(self.handle) } {
            warn!(
                ?error,
                setting = self.name,
                "Failed to unregister standby power setting notification"
            );
        }
    }
}

struct CallbackStandbySubscription {
    handle: HPOWERNOTIFY,
    _params: Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>,
}

impl CallbackStandbySubscription {
    fn register() -> Option<Self> {
        let mut params = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(standby_callback),
            Context: null_mut(),
        });
        let mut handle = HPOWERNOTIFY::default();
        let result = unsafe {
            PowerRegisterSuspendResumeNotification(
                DEVICE_NOTIFY_CALLBACK,
                HANDLE((&mut *params) as *mut _ as *mut _),
                &mut handle.0 as *mut _ as *mut _,
            )
        };
        if result.is_err() {
            warn!(
                code = result.0,
                "Power suspend/resume callback registration failed"
            );
            return None;
        }

        Some(Self {
            handle,
            _params: params,
        })
    }
}

impl Drop for CallbackStandbySubscription {
    fn drop(&mut self) {
        let result = unsafe { PowerUnregisterSuspendResumeNotification(self.handle) };
        if result.is_err() {
            warn!(
                code = result.0,
                "Failed to unregister standby callback notification"
            );
        }
    }
}

struct StandbySubscriptions {
    _window: Option<WindowStandbySubscription>,
    _callback: Option<CallbackStandbySubscription>,
    _session: Option<SessionStandbySubscription>,
    _power_settings: Option<PowerSettingStandbySubscription>,
}

impl StandbySubscriptions {
    fn register(hwnd: HWND) -> Self {
        let window = WindowStandbySubscription::register(hwnd);
        if window.is_some() {
            info!("Registered standby window notification");
        } else {
            warn!("Standby window notification registration failed");
        }

        let callback = CallbackStandbySubscription::register();
        if callback.is_some() {
            info!("Registered standby callback notification");
        }

        let session = SessionStandbySubscription::register(hwnd);
        if session.is_some() {
            info!("Registered standby session notification");
        } else {
            warn!("Standby session notification registration failed");
        }

        let power_settings = PowerSettingStandbySubscription::register(hwnd);
        if power_settings.is_none() {
            warn!("No standby power setting notifications could be registered");
        }

        Self {
            _window: window,
            _callback: callback,
            _session: session,
            _power_settings: power_settings,
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
    fn standby_event_mapping_updates_state_for_interactive_resume() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        assert!(update_standby_state_from_event(PBT_APMRESUMESUSPEND));
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

    #[test]
    fn resume_gap_detection_uses_threshold() {
        assert!(!resume_gap_detected(
            STANDBY_RESUME_GAP_THRESHOLD - Duration::from_millis(1)
        ));
        assert!(resume_gap_detected(STANDBY_RESUME_GAP_THRESHOLD));
    }

    #[test]
    fn watchdog_resume_sets_wake_from_normal() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        *standby_state() = StandbyState::Normal;

        assert!(update_standby_state_from_watchdog_resume());
        assert_eq!(*standby_state(), StandbyState::Wake);
    }

    #[test]
    fn session_unlock_maps_to_wake() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        *standby_state() = StandbyState::Normal;

        assert!(session_change_is_wake(WTS_SESSION_UNLOCK_EVENT));
        assert!(update_standby_state_from_session_resume());
        assert_eq!(*standby_state(), StandbyState::Wake);
    }

    #[test]
    fn display_power_settings_map_off_to_sleep_and_on_to_wake() {
        assert_eq!(
            display_power_setting_state(GUID_CONSOLE_DISPLAY_STATE, DISPLAY_POWER_OFF),
            Some(("console_display_state", StandbyState::Sleep))
        );
        assert_eq!(
            display_power_setting_state(GUID_SESSION_DISPLAY_STATUS, DISPLAY_POWER_ON),
            Some(("session_display_status", StandbyState::Wake))
        );
        assert_eq!(
            display_power_setting_state(GUID_MONITOR_POWER_ON, DISPLAY_POWER_OFF),
            Some(("monitor_power_on", StandbyState::Sleep))
        );
    }

    #[test]
    fn display_power_settings_ignore_dimmed_and_unknown_settings() {
        assert_eq!(
            display_power_setting_state(GUID_CONSOLE_DISPLAY_STATE, 2),
            None
        );
        assert_eq!(
            display_power_setting_state(windows::core::GUID::zeroed(), DISPLAY_POWER_OFF),
            None
        );
    }

    #[test]
    fn display_power_state_updates_standby_state() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        *standby_state() = StandbyState::Normal;

        assert!(update_standby_state_from_display_power(StandbyState::Sleep));
        assert_eq!(*standby_state(), StandbyState::Sleep);
        assert!(!update_standby_state_from_display_power(
            StandbyState::Sleep
        ));

        assert!(update_standby_state_from_display_power(StandbyState::Wake));
        assert_eq!(*standby_state(), StandbyState::Wake);
    }

    #[test]
    fn join_standby_watchdog_thread_drains_handle() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *standby_watchdog_thread() = Some(thread::spawn(|| {}));

        join_standby_watchdog_thread();

        assert!(standby_watchdog_thread().is_none());
    }
}
