use crate::razer::device_state::*;
use crate::razer::executer::Executer;
use crate::win::actions::AudioType;
use crate::win::persist::PersistBuffer;

use librazer::device::Device;
use std::sync::OnceLock;
use std::time::Duration;

use std::sync::mpsc::{self, Sender};
use std::thread;

static CMDS_TX: OnceLock<mpsc::Sender<DeviceCmd>> = OnceLock::new();

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
    PersistConfig,
    Shutdown,
}

pub struct DeviceHandle {
    sender: Sender<DeviceCmd>,
}

impl DeviceHandle {
    fn get<T, F>(&self, make_query: F) -> anyhow::Result<T>
    where
        F: FnOnce(mpsc::Sender<T>) -> DeviceCmd,
    {
        let (resp_tx, resp_rx) = mpsc::channel::<T>();

        // Construct the command (e.g., DeviceCmd::GetKeyboardBrightness(resp_tx))
        let cmd = make_query(resp_tx);

        self.sender
            .send(cmd)
            .expect("Device handle thread is no longer running");

        // Block until the handle sends T back
        let resp = resp_rx
            .recv_timeout(Duration::from_secs(1))
            .map_err(|_| anyhow::anyhow!("Hardware response timeout"))?;

        Ok(resp)
    }
    pub fn keyboard_light_up(&self) {
        let _ = self.sender.send(DeviceCmd::AdjustKeyboardLight(true));
    }
    pub fn keyboard_light_down(&self) {
        let _ = self.sender.send(DeviceCmd::AdjustKeyboardLight(false));
    }
    pub fn get_pid(&self) -> u16 {
        self.get(DeviceCmd::GetPID)
            .expect("Device PID config error")
    }
    pub fn get_perf_mode(&self) -> PerfMode {
        self.get(DeviceCmd::GetPerfMode)
            .expect("Get performance mode error")
    }
    pub fn initialize(&self) {
        let _ = self.sender.send(DeviceCmd::InitializeDevice);
    }
    pub fn sleep(&self) {
        let _ = self.sender.send(DeviceCmd::SleepDevice);
    }
    pub fn set_speakers_mute_indicator(&self, muted: bool) {
        let _ = self
            .sender
            .send(DeviceCmd::SetMuteIndicator(AudioType::Speakers, muted));
    }
    pub fn set_mic_mute_indicator(&self, muted: bool) {
        let _ = self
            .sender
            .send(DeviceCmd::SetMuteIndicator(AudioType::Mic, muted));
    }
    pub fn cycle_rgb_mode(&self) {
        let _ = self.sender.send(DeviceCmd::CycleRGBMode);
    }
    pub fn cycle_perf_mode(&self) {
        let _ = self.sender.send(DeviceCmd::CyclePerfMode);
    }
    pub fn toggle_vc(&self) {
        let _ = self.sender.send(DeviceCmd::ToggleUnderGlow);
    }
    pub fn adjust_screen_brightness(&self, change: i8) {
        let _ = self.sender.send(DeviceCmd::AdjustScreenBrightness(change));
    }
    pub fn persist_config(&self) {
        let _ = self.sender.send(DeviceCmd::PersistConfig);
    }
    pub fn shutdown(&self) {
        let _ = self.sender.send(DeviceCmd::Shutdown);
    }
}

pub fn device() -> DeviceHandle {
    let tx = CMDS_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<DeviceCmd>();

        thread::spawn(move || {
            let device = Device::detect().expect("No compatible device found");
            let mut app_config = load_config();
            let persist_beffer = PersistBuffer::new(CONFIG_PATH.to_string());
            Executer::new(&device, &mut app_config, persist_beffer, rx).process_commands();
        });

        tx
    });

    DeviceHandle { sender: tx.clone() }
}
