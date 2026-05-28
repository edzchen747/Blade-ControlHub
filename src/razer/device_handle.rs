use crate::config::{self, CONFIG_PATH};
use crate::core::shared_state::DEVICE_PIDS;
use crate::error::{AppError, AppResult};
use crate::razer::config::AppConfig;
use crate::razer::enums::{LidLogoMode, PerfMode};
use crate::razer::executer::Executer;
use crate::razer::protocol::command;
use crate::utils::persist::PersistBuffer;
use crate::utils::reload::restart_app;
use crate::win::audio::AudioType;
use librazer::descriptor::Descriptor;
use tracing::{error, info, warn};

use librazer::device::Device;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

static MODEL: OnceLock<String> = OnceLock::new();

// ── Global Singleton ────────────────────────────────────────────────────────

static CMDS_TX: OnceLock<mpsc::Sender<DeviceCmd>> = OnceLock::new();

/// Returns a `DeviceHandle` connected to the background device thread.
/// On first call, spawns the worker thread that owns the hardware device.
pub fn device() -> DeviceHandle {
    let tx = CMDS_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<DeviceCmd>();

        thread::spawn(move || {
            let device = get_razer_device();
            let mut app_config = config::load_config(format!("0x{:04x}", device.info.pid));
            let persist_buffer = PersistBuffer::new(CONFIG_PATH.to_string());
            let mut executer = match Executer::new(&device, &mut app_config, persist_buffer, rx) {
                Ok(e) => e,
                Err(e) => {
                    error!(error = %e, "Failed to initialise Executer; worker thread exiting");
                    return;
                }
            };
            executer.process_commands();
        });
        monitor_device_channel();

        tx
    });

    DeviceHandle { sender: tx.clone() }
}

fn monitor_device_channel() {
    std::thread::spawn(|| {
        info!("Started channel monitor thread");
        std::thread::sleep(std::time::Duration::from_secs(20));
        loop {
            let _ = device().get_pid();
            std::thread::sleep(std::time::Duration::from_mins(2));
        }
    });
}

fn get_razer_device() -> Device {
    match Device::detect() {
        Ok(d) => d,
        Err(e) => {
            if let Ok(mut razer_enums) = Device::enumerate() {
                razer_enums.0.sort();
                let (device_pids, device_model) = razer_enums;
                let model = MODEL.get_or_init(|| format!("Razer Blade {}", device_model));
                DEVICE_PIDS
                    .set(device_pids.clone())
                    .expect("Fatal internal error: failed to set device pids");

                // Loop through all detected Razer PIDs and return the first that responds
                for pid in &device_pids {
                    let custom_descriptor = Descriptor {
                        model_number_prefix: &model,
                        name: &model,
                        pid: *pid,
                        features: &[],
                    };
                    let device = Device::new(custom_descriptor)
                        .expect("Fatal internal error: failed to initialize custom descriptor");
                    // Command to check performance mode
                    if command(&device, 0x0d82, &[0, 0, 0, 0], Some(2)).is_ok() {
                        return device;
                    }
                }
                error!(error = ?e, "No compatible Razer device found; worker thread exiting");
                std::process::exit(1);
            } else {
                error!(error = ?e, "No compatible Razer device found; worker thread exiting");
                std::process::exit(1);
            }
        }
    }
}

// ── Device Commands ─────────────────────────────────────────────────────────

/// Commands that can be sent to the background device thread.
pub enum DeviceCmd {
    InitializeDevice(bool),
    SleepDevice,
    AdjustKeyboardLight(bool),
    GetPID(mpsc::Sender<u16>),
    GetModelName(mpsc::Sender<String>),
    #[allow(dead_code)]
    GetPerfMode(mpsc::Sender<PerfMode>),
    GetDefaultMultimediaKeys(mpsc::Sender<bool>),
    ToggleDefaultMultimediaKeys(mpsc::Sender<bool>),
    SetMuteIndicator(AudioType, bool),
    CycleBatteryLimit,
    CycleRGBMode,
    CyclePerfMode,
    ToggleUnderGlow,
    AdjustScreenBrightness(i8),
    CycleRefreshRate,
    SetKeyboardColor(u8, u8, u8),
    #[allow(dead_code)]
    SetLidLogo(LidLogoMode),
    PersistConfig,
    GetConfig(mpsc::Sender<AppConfig>),
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

    pub fn get_config(&self) -> AppResult<AppConfig> {
        self.query(DeviceCmd::GetConfig)
    }

    pub fn shutdown(&self) -> AppResult<bool> {
        self.query(DeviceCmd::Shutdown)
    }

    // ── Fire-and-forget commands ────────────────────────────────────

    pub fn initialize(&self, notify_startup: bool) {
        self.send(DeviceCmd::InitializeDevice(notify_startup));
    }

    pub fn sleep(&self) {
        self.send(DeviceCmd::SleepDevice);
    }

    pub fn keyboard_light_up(&self) {
        self.send(DeviceCmd::AdjustKeyboardLight(true));
    }

    pub fn keyboard_light_down(&self) {
        self.send(DeviceCmd::AdjustKeyboardLight(false));
    }

    pub fn set_keyboard_color(&self, r: u8, g: u8, b: u8) {
        self.send(DeviceCmd::SetKeyboardColor(r, g, b));
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

    pub fn cycle_perf_mode(&self) {
        self.send(DeviceCmd::CyclePerfMode);
    }

    pub fn toggle_vc(&self) {
        self.send(DeviceCmd::ToggleUnderGlow);
    }

    pub fn adjust_screen_brightness(&self, change: i8) {
        self.send(DeviceCmd::AdjustScreenBrightness(change));
    }

    pub fn cycle_refresh_rate(&self) {
        self.send(DeviceCmd::CycleRefreshRate);
    }

    pub fn cycle_battery_limit(&self) {
        self.send(DeviceCmd::CycleBatteryLimit);
    }

    pub fn persist_config(&self) {
        self.send(DeviceCmd::PersistConfig);
    }

    // ── Internal helpers ────────────────────────────────────────────

    /// Sends a command and logs an error if the device thread has exited.
    fn send(&self, cmd: DeviceCmd) {
        if self.sender.send(cmd).is_err() {
            warn!("Device executer unresponsive; restarting app");
            restart_app(1);
        }
    }

    /// Sends a query command and blocks until the response arrives (5s timeout).
    fn query<T, F>(&self, make_query: F) -> AppResult<T>
    where
        F: FnOnce(mpsc::Sender<T>) -> DeviceCmd,
    {
        let (resp_tx, resp_rx) = mpsc::channel::<T>();
        let cmd = make_query(resp_tx);

        if self.sender.send(cmd).is_err() {
            warn!("Device executer unresponsive; restarting app");
            restart_app(1);
        }

        resp_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| AppError::HardwareTimeout)
    }
}

// ── DeviceController Trait Implementation ─────────────────────────────────────

impl crate::core::traits::DeviceController for DeviceHandle {
    fn initialize(&self, notify_startup: bool) {
        DeviceHandle::initialize(self, notify_startup);
    }

    fn sleep(&self) {
        DeviceHandle::sleep(self);
    }

    fn shutdown(&self) -> crate::error::AppResult<bool> {
        DeviceHandle::shutdown(self)
    }

    fn get_pid(&self) -> crate::error::AppResult<u16> {
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

    fn set_lid_logo(&self, mode: crate::razer::enums::LidLogoMode) {
        DeviceHandle::set_lid_logo(self, mode);
    }

    fn persist_config(&self) {
        DeviceHandle::persist_config(self);
    }
}
