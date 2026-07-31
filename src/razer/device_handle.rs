use crate::config::ThemeColor;
use crate::config::{self, CONFIG_PATH};
use crate::core::shared_state::DEVICE_PIDS;
use crate::error::{AppError, AppResult};
use crate::razer::config::{AppConfig, PowerProfile};
use crate::razer::enums::{BatteryLimit, LidLogoMode, PerfMode, RGBEffect};
use crate::razer::executer::Executer;
use crate::razer::protocol::command;
use crate::runtime::settings_state::SettingsState;
use crate::utils::persist::PersistBuffer;
use crate::win::audio::AudioType;
use librazer::descriptor::Descriptor;
use tracing::{error, info, warn};

use librazer::device::Device;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

static MODEL: OnceLock<String> = OnceLock::new();
static DEVICE_CHANNEL_MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);
static DEVICE_CHANNEL_MONITOR_WAKE: OnceLock<Arc<DeviceChannelMonitorWake>> = OnceLock::new();
static DEVICE_CHANNEL_MONITOR_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

type DeviceChannelMonitorWake = (Mutex<()>, Condvar);

// ── Global Singleton ────────────────────────────────────────────────────────

static CMDS_TX: OnceLock<mpsc::Sender<DeviceCmd>> = OnceLock::new();

/// Returns a `DeviceHandle` connected to the background device thread.
/// On first call, spawns the worker thread that owns the hardware device.
pub fn device() -> DeviceHandle {
    let tx = CMDS_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<DeviceCmd>();

        if let Err(error) = thread::Builder::new()
            .name("blade-device-worker".to_string())
            .spawn(move || run_device_worker(rx))
        {
            error!(%error, "Failed to start Razer device worker thread");
        }
        monitor_device_channel();

        tx
    });

    DeviceHandle { sender: tx.clone() }
}

fn monitor_device_channel() {
    join_finished_device_channel_monitor_thread();

    if DEVICE_CHANNEL_MONITOR_RUNNING.swap(true, Ordering::SeqCst) {
        warn!("Device channel monitor is already running");
        return;
    }

    match thread::Builder::new()
        .name("blade-device-channel-monitor".to_string())
        .spawn(|| {
            info!("Started channel monitor thread");
            wait_for_device_channel_monitor(Duration::from_secs(20));
            while DEVICE_CHANNEL_MONITOR_RUNNING.load(Ordering::SeqCst) {
                let _ = device().get_pid();
                wait_for_device_channel_monitor(Duration::from_mins(2));
            }
        }) {
        Ok(handle) => {
            *device_channel_monitor_thread() = Some(handle);
        }
        Err(error) => {
            DEVICE_CHANNEL_MONITOR_RUNNING.store(false, Ordering::SeqCst);
            error!(%error, "Failed to start device channel monitor thread");
        }
    }
}

pub fn stop_device_channel_monitor() {
    DEVICE_CHANNEL_MONITOR_RUNNING.store(false, Ordering::SeqCst);
    device_channel_monitor_wake().1.notify_all();
    join_device_channel_monitor_thread();
}

fn device_channel_monitor_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    DEVICE_CHANNEL_MONITOR_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_device_channel_monitor_thread() {
    let should_join = device_channel_monitor_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);

    if should_join {
        join_device_channel_monitor_thread();
    }
}

fn join_device_channel_monitor_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = device_channel_monitor_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current device channel monitor thread during shutdown");
        *device_channel_monitor_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("Device channel monitor thread panicked during shutdown");
    }
}

fn device_channel_monitor_wake() -> Arc<DeviceChannelMonitorWake> {
    DEVICE_CHANNEL_MONITOR_WAKE
        .get_or_init(|| Arc::new((Mutex::new(()), Condvar::new())))
        .clone()
}

fn wait_for_device_channel_monitor(duration: Duration) {
    let signal = device_channel_monitor_wake();
    let (lock, cvar) = &*signal;
    let guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (_guard, _timeout) = cvar
        .wait_timeout_while(guard, duration, |_| {
            DEVICE_CHANNEL_MONITOR_RUNNING.load(Ordering::SeqCst)
        })
        .unwrap_or_else(|poisoned| poisoned.into_inner());
}

fn run_device_worker(rx: mpsc::Receiver<DeviceCmd>) {
    let device = match get_razer_device() {
        Ok(device) => device,
        Err(error) => {
            error!(%error, "No compatible Razer device found; worker thread exiting");
            return;
        }
    };
    let mut app_config = config::load_config(&device.info);
    let persist_buffer = PersistBuffer::new(CONFIG_PATH.to_string());
    let mut executer = match Executer::new(&device, &mut app_config, persist_buffer, rx) {
        Ok(e) => e,
        Err(e) => {
            error!(error = %e, "Failed to initialise Executer; worker thread exiting");
            return;
        }
    };
    executer.process_commands();
}

fn get_razer_device() -> AppResult<Device> {
    match Device::detect() {
        Ok(d) => Ok(d),
        Err(e) => {
            if let Ok(mut razer_enums) = Device::enumerate() {
                razer_enums.0.sort();
                let (device_pids, device_model) = razer_enums;
                let model = MODEL.get_or_init(|| format!("Razer Blade {}", device_model));
                if DEVICE_PIDS.set(device_pids.clone()).is_err() {
                    warn!("Device PID cache was already initialized");
                }

                // Loop through all detected Razer PIDs and return the first that responds
                for pid in &device_pids {
                    let custom_descriptor = Descriptor {
                        model_number_prefix: model,
                        name: model,
                        pid: *pid,
                        features: &[],
                    };
                    let Ok(device) = Device::new(custom_descriptor) else {
                        continue;
                    };
                    // Command to check performance mode
                    if command(&device, 0x0d82, &[0, 0, 0, 0], Some(2)).is_ok() {
                        return Ok(device);
                    }
                }
                Err(AppError::Internal(format!(
                    "No responding Razer HID interface found after fallback detection: {e}"
                )))
            } else {
                Err(AppError::Internal(format!(
                    "Razer device detection and enumeration failed: {e}"
                )))
            }
        }
    }
}

// ── Device Commands ─────────────────────────────────────────────────────────

/// Commands that can be sent to the background device thread.
pub enum DeviceCmd {
    InitializeDevice(bool),
    SleepDevice(mpsc::Sender<bool>),
    AdjustKeyboardLight(bool),
    GetPID(mpsc::Sender<u16>),
    GetModelName(mpsc::Sender<String>),
    #[allow(dead_code)]
    GetPerfMode(mpsc::Sender<PerfMode>),
    GetDefaultMultimediaKeys(mpsc::Sender<bool>),
    ToggleDefaultMultimediaKeys(mpsc::Sender<bool>),
    SetDefaultMultimediaKeys(bool, mpsc::Sender<AppResult<()>>),
    SetMuteIndicator(AudioType, bool),
    CycleBatteryLimit,
    SetBatteryLimit(BatteryLimit, mpsc::Sender<AppResult<()>>),
    SetThemeColor(ThemeColor, mpsc::Sender<AppResult<()>>),
    CycleRGBMode,
    SetRGBMode(PowerProfile, RGBEffect, mpsc::Sender<AppResult<()>>),
    CyclePerfMode,
    SetPerfMode(PowerProfile, PerfMode, mpsc::Sender<AppResult<()>>),
    ToggleUnderGlow,
    SetUnderGlow(PowerProfile, bool, mpsc::Sender<AppResult<()>>),
    AdjustScreenBrightness(i8),
    CycleRefreshRate,
    SetRefreshRate(PowerProfile, u32, mpsc::Sender<AppResult<()>>),
    DisplayLayoutChanged,
    SetKeyboardBrightness(PowerProfile, u8, mpsc::Sender<AppResult<()>>),
    SetKeyboardColor(u8, u8, u8, u8),
    #[allow(dead_code)]
    SetLidLogo(LidLogoMode),
    PersistConfig,
    GetConfig(mpsc::Sender<AppConfig>),
    GetSettingsState(mpsc::Sender<SettingsState>),
    Shutdown(mpsc::Sender<bool>),
}

// ── DeviceHandle ────────────────────────────────────────────────────────────

/// A thread-safe, cloneable handle for sending commands to the device thread.
#[derive(Debug, Clone)]
pub struct DeviceHandle {
    sender: Sender<DeviceCmd>,
}

impl DeviceHandle {
    // ── Queries (blocking) ──────────────────────────────────────────

    pub fn get_pid(&self) -> AppResult<u16> {
        self.query(DeviceCmd::GetPID)
    }

    pub fn get_model_name(&self) -> AppResult<String> {
        self.query(DeviceCmd::GetModelName)
    }

    #[allow(dead_code)]
    pub fn get_perf_mode(&self) -> AppResult<PerfMode> {
        self.query(DeviceCmd::GetPerfMode)
    }

    #[allow(dead_code)]
    pub fn get_default_multimedia_keys(&self) -> AppResult<bool> {
        self.query(DeviceCmd::GetDefaultMultimediaKeys)
    }

    pub fn toggle_default_multimedia_keys(&self) -> AppResult<bool> {
        self.query(DeviceCmd::ToggleDefaultMultimediaKeys)
    }

    pub fn set_default_multimedia_keys(&self, enabled: bool) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetDefaultMultimediaKeys(enabled, tx))
    }

    pub fn get_config(&self) -> AppResult<AppConfig> {
        self.query(DeviceCmd::GetConfig)
    }

    pub fn get_settings_state(&self) -> AppResult<SettingsState> {
        self.query(DeviceCmd::GetSettingsState)
    }

    pub fn shutdown(&self) -> AppResult<bool> {
        self.query(DeviceCmd::Shutdown)
    }

    pub fn sleep(&self) -> AppResult<bool> {
        self.query(DeviceCmd::SleepDevice)
    }

    // ── Fire-and-forget commands ────────────────────────────────────

    pub fn initialize(&self, notify_startup: bool) {
        self.send(DeviceCmd::InitializeDevice(notify_startup));
    }

    pub fn keyboard_light_up(&self) {
        self.send(DeviceCmd::AdjustKeyboardLight(true));
    }

    pub fn keyboard_light_down(&self) {
        self.send(DeviceCmd::AdjustKeyboardLight(false));
    }

    pub fn set_keyboard_color(&self, r: u8, g: u8, b: u8, brightness: u8) {
        self.send(DeviceCmd::SetKeyboardColor(r, g, b, brightness));
    }

    pub fn set_keyboard_brightness(&self, profile: PowerProfile, brightness: u8) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetKeyboardBrightness(profile, brightness, tx))
    }

    #[allow(dead_code)]
    pub fn set_lid_logo(&self, mode: LidLogoMode) {
        self.send(DeviceCmd::SetLidLogo(mode));
    }

    pub fn set_speakers_mute_indicator(&self, muted: bool) {
        self.send(DeviceCmd::SetMuteIndicator(AudioType::Speakers, muted));
    }

    pub fn set_mic_mute_indicator(&self, muted: bool) {
        self.send(DeviceCmd::SetMuteIndicator(AudioType::Mic, muted));
    }

    pub fn cycle_rgb_mode(&self) {
        self.send(DeviceCmd::CycleRGBMode);
    }

    pub fn set_rgb_mode(&self, profile: PowerProfile, effect: RGBEffect) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetRGBMode(profile, effect, tx))
    }

    pub fn cycle_perf_mode(&self) {
        self.send(DeviceCmd::CyclePerfMode);
    }

    pub fn set_perf_mode(&self, profile: PowerProfile, mode: PerfMode) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetPerfMode(profile, mode, tx))
    }

    pub fn toggle_vc(&self) {
        self.send(DeviceCmd::ToggleUnderGlow);
    }

    pub fn set_under_glow(&self, profile: PowerProfile, enabled: bool) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetUnderGlow(profile, enabled, tx))
    }

    pub fn adjust_screen_brightness(&self, change: i8) {
        self.send(DeviceCmd::AdjustScreenBrightness(change));
    }

    pub fn cycle_refresh_rate(&self) {
        self.send(DeviceCmd::CycleRefreshRate);
    }

    pub fn set_refresh_rate(&self, profile: PowerProfile, refresh_rate: u32) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetRefreshRate(profile, refresh_rate, tx))
    }

    pub fn display_layout_changed(&self) {
        self.send(DeviceCmd::DisplayLayoutChanged);
    }

    pub fn cycle_battery_limit(&self) {
        self.send(DeviceCmd::CycleBatteryLimit);
    }

    pub fn set_battery_limit(&self, limit: BatteryLimit) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetBatteryLimit(limit, tx))
    }

    pub fn set_theme_color(&self, color: ThemeColor) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetThemeColor(color, tx))
    }

    pub fn persist_config(&self) {
        self.send(DeviceCmd::PersistConfig);
    }

    // ── Internal helpers ────────────────────────────────────────────

    /// Sends a command and logs an error if the device thread has exited.
    fn send(&self, cmd: DeviceCmd) {
        if let Err(error) = self.sender.send(cmd) {
            warn!(?error, "Device worker is unavailable; dropping command");
        }
    }

    /// Sends a query command and blocks until the response arrives (5s timeout).
    fn query<T, F>(&self, make_query: F) -> AppResult<T>
    where
        F: FnOnce(mpsc::Sender<T>) -> DeviceCmd,
    {
        let (resp_tx, resp_rx) = mpsc::channel::<T>();
        let cmd = make_query(resp_tx);

        if let Err(error) = self.sender.send(cmd) {
            warn!(
                ?error,
                "Device worker is unavailable; query cannot be delivered"
            );
            return Err(device_worker_unavailable());
        }

        match resp_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(value) => Ok(value),
            Err(RecvTimeoutError::Timeout) => Err(AppError::HardwareTimeout),
            Err(RecvTimeoutError::Disconnected) => Err(device_worker_unavailable()),
        }
    }

    fn query_result<T, F>(&self, make_query: F) -> AppResult<T>
    where
        F: FnOnce(mpsc::Sender<AppResult<T>>) -> DeviceCmd,
    {
        let (resp_tx, resp_rx) = mpsc::channel::<AppResult<T>>();
        let cmd = make_query(resp_tx);

        if let Err(error) = self.sender.send(cmd) {
            warn!(
                ?error,
                "Device worker is unavailable; query cannot be delivered"
            );
            return Err(device_worker_unavailable());
        }

        match resp_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(AppError::HardwareTimeout),
            Err(RecvTimeoutError::Disconnected) => Err(device_worker_unavailable()),
        }
    }
}

fn device_worker_unavailable() -> AppError {
    AppError::Internal("Device worker is unavailable".to_string())
}

// ── DeviceController Trait Implementation ─────────────────────────────────────

impl crate::hal::DeviceController for DeviceHandle {
    fn initialize(&self, notify_startup: bool) {
        DeviceHandle::initialize(self, notify_startup);
    }

    fn sleep(&self) -> AppResult<bool> {
        DeviceHandle::sleep(self)
    }

    fn shutdown(&self) -> AppResult<bool> {
        DeviceHandle::shutdown(self)
    }

    fn get_pid(&self) -> AppResult<u16> {
        DeviceHandle::get_pid(self)
    }

    fn cycle_perf_mode(&self) {
        DeviceHandle::cycle_perf_mode(self);
    }

    fn cycle_rgb_mode(&self) {
        DeviceHandle::cycle_rgb_mode(self);
    }

    fn cycle_refresh_rate(&self) {
        DeviceHandle::cycle_refresh_rate(self);
    }

    fn cycle_battery_limit(&self) {
        DeviceHandle::cycle_battery_limit(self);
    }

    fn toggle_vc(&self) {
        DeviceHandle::toggle_vc(self);
    }

    fn keyboard_light_up(&self) {
        DeviceHandle::keyboard_light_up(self);
    }

    fn keyboard_light_down(&self) {
        DeviceHandle::keyboard_light_down(self);
    }

    fn adjust_screen_brightness(&self, change: i8) {
        DeviceHandle::adjust_screen_brightness(self, change);
    }

    fn set_lid_logo(&self, mode: LidLogoMode) {
        DeviceHandle::set_lid_logo(self, mode);
    }

    fn persist_config(&self) {
        DeviceHandle::persist_config(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed_handle() -> DeviceHandle {
        let (tx, rx) = mpsc::channel::<DeviceCmd>();
        drop(rx);
        DeviceHandle { sender: tx }
    }

    #[test]
    fn query_returns_internal_when_worker_channel_is_closed() {
        let result = closed_handle().get_pid();

        assert!(matches!(
            result,
            Err(AppError::Internal(message)) if message.contains("Device worker is unavailable")
        ));
    }

    #[test]
    fn fire_and_forget_command_does_not_panic_when_worker_channel_is_closed() {
        let handle = closed_handle();

        handle.initialize(false);
    }

    #[test]
    fn stop_device_channel_monitor_clears_running_flag() {
        DEVICE_CHANNEL_MONITOR_RUNNING.store(true, Ordering::SeqCst);

        stop_device_channel_monitor();

        assert!(!DEVICE_CHANNEL_MONITOR_RUNNING.load(Ordering::SeqCst));
    }

    #[test]
    fn join_device_channel_monitor_thread_drains_handle() {
        *device_channel_monitor_thread() = Some(thread::spawn(|| {}));

        join_device_channel_monitor_thread();

        assert!(device_channel_monitor_thread().is_none());
    }

    #[test]
    fn query_returns_internal_when_worker_drops_response_sender() {
        let (tx, rx) = mpsc::channel::<DeviceCmd>();
        let handle = DeviceHandle { sender: tx };
        let worker = thread::spawn(move || {
            let _ = rx.recv();
        });

        let result = handle.get_pid();
        worker.join().expect("test worker must exit");

        assert!(matches!(
            result,
            Err(AppError::Internal(message)) if message.contains("Device worker is unavailable")
        ));
    }
}
