use crate::razer::config::*;
use crate::razer::enums::PerfMode;
use crate::razer::executer::Executer;
use crate::utils::persist::PersistBuffer;
use crate::win::audio::AudioType;

use librazer::device::Device;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

// ── Global Singleton ────────────────────────────────────────────────────────

static CMDS_TX: OnceLock<mpsc::Sender<DeviceCmd>> = OnceLock::new();

/// Returns a `DeviceHandle` connected to the background device thread.
/// On first call, spawns the worker thread that owns the hardware device.
pub fn device() -> DeviceHandle {
    let tx = CMDS_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<DeviceCmd>();

        thread::spawn(move || {
            let device = Device::detect().expect("No compatible device found");
            let mut app_config = load_config();
            let persist_buffer = PersistBuffer::new(CONFIG_PATH.to_string());
            Executer::new(&device, &mut app_config, persist_buffer, rx).process_commands();
        });

        tx
    });

    DeviceHandle { sender: tx.clone() }
}

// ── Device Commands ─────────────────────────────────────────────────────────

/// Commands that can be sent to the background device thread.
pub enum DeviceCmd {
    InitializeDevice,
    SleepDevice,
    AdjustKeyboardLight(bool),
    GetPID(mpsc::Sender<u16>),
    GetPerfMode(mpsc::Sender<PerfMode>),
    SetMuteIndicator(AudioType, bool),
    CycleRGBMode,
    CyclePerfMode,
    ToggleUnderGlow,
    AdjustScreenBrightness(i8),
    SetScreenBrightness(u8),
    GetRefreshRate(mpsc::Sender<u32>),
    CycleRefreshRate,
    KeyboardColor(u8, u8, u8),
    PersistConfig,
    Shutdown,
}

// ── DeviceHandle ────────────────────────────────────────────────────────────

/// A thread-safe, cloneable handle for sending commands to the device thread.
#[derive(Debug, Clone)]
pub struct DeviceHandle {
    sender: Sender<DeviceCmd>,
}

impl DeviceHandle {
    // ── Queries (blocking) ──────────────────────────────────────────

    pub fn get_pid(&self) -> u16 {
        self.query(DeviceCmd::GetPID)
            .expect("Device PID config error")
    }

    pub fn get_perf_mode(&self) -> PerfMode {
        self.query(DeviceCmd::GetPerfMode)
            .expect("Get performance mode error")
    }

    pub fn get_refresh_rate(&self) -> u32 {
        self.query(DeviceCmd::GetRefreshRate)
            .expect("Get refresh rate error")
    }

    // ── Fire-and-forget commands ────────────────────────────────────

    pub fn initialize(&self) {
        self.send(DeviceCmd::InitializeDevice);
    }

    pub fn sleep(&self) {
        self.send(DeviceCmd::SleepDevice);
    }

    pub fn shutdown(&self) {
        self.send(DeviceCmd::Shutdown);
    }

    pub fn keyboard_light_up(&self) {
        self.send(DeviceCmd::AdjustKeyboardLight(true));
    }

    pub fn keyboard_light_down(&self) {
        self.send(DeviceCmd::AdjustKeyboardLight(false));
    }

    pub fn keyboard_color(&self, r: u8, g: u8, b: u8) {
        self.send(DeviceCmd::KeyboardColor(r, g, b));
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

    pub fn persist_config(&self) {
        self.send(DeviceCmd::PersistConfig);
    }

    // ── Internal helpers ────────────────────────────────────────────

    /// Sends a command and ignores the result.
    fn send(&self, cmd: DeviceCmd) {
        let _ = self.sender.send(cmd);
    }

    /// Sends a query command and blocks until the response arrives (1s timeout).
    fn query<T, F>(&self, make_query: F) -> anyhow::Result<T>
    where
        F: FnOnce(mpsc::Sender<T>) -> DeviceCmd,
    {
        let (resp_tx, resp_rx) = mpsc::channel::<T>();
        let cmd = make_query(resp_tx);

        self.sender
            .send(cmd)
            .expect("Device handle thread is no longer running");

        resp_rx
            .recv_timeout(Duration::from_secs(1))
            .map_err(|_| anyhow::anyhow!("Hardware response timeout"))
    }
}
