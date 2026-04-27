use crate::win::actions::AudioType;

use librazer::device::{Device};
use librazer::packet::Packet;
use std::sync::{OnceLock, Mutex};

struct DeviceState {
    last_key_lvl: u8,
    last_keyboard_rgb: u32,
    last_vc_active: bool
}

impl DeviceState {
    const fn new() -> Self {
        Self {
            last_key_lvl: 255,
            last_keyboard_rgb: 0,
            last_vc_active: true
        }
    }

    pub fn rgb_code(&self) -> u8 {
        RGB_VALS[(self.last_keyboard_rgb % RGB_VALS.len() as u32) as usize]
    }
}

static RGB_VALS: [u8; 3] = [4, 1, 3];

static STATE_PWR: Mutex<DeviceState> = Mutex::new(DeviceState::new());
static STATE_BATT: Mutex<DeviceState> = Mutex::new(DeviceState::new());

pub fn command(device: &Device, command: u16, args: &[u8]) -> anyhow::Result<u8> {
    for _attempt in 1..=3 {
        let report = Packet::new(command, args);
        let response = device.send(report)?;
        if response.get_args().len() >= args.len() && 
        response.get_args().iter()
            .enumerate()
            .take(args.len())
            .filter(|&(i, _)| i != 2) // SKIP index 2
            .all(|(i, &byte)| byte == args[i])
        {
            return Ok(response.get_args()[2]);
        }
    }
    Ok(0)
}

pub fn initialize_keyboard(device: &Device) {
    let _ = command(device, 0x0004, &[3, 0]); // turn on keyboard
    let _ = command(device, 0x0206, &[0, 1]); // set multi media keys if configured
    let _ = command(device, 0x0086, &[0, 0, 0]); // set multi media keys if configured
    let _ = command(device, 0x030a, &[STATE_PWR.lock().unwrap().rgb_code(), 0]); // set effect
    // let _ = command(device, 0x0300, &[1, 38, STATE_PWR.lock().unwrap().last_vc_active as u8]);
    // let _ = command(device, 0x0303, &[1, 38, (STATE_PWR.lock().unwrap().last_vc_active as u8) * 255]);
    let _ = set_keyboard_brightness(device, STATE_PWR.lock().unwrap().last_key_lvl); // set brightness
}

fn keyboard_light(device: &Device, up: bool) -> () {
    let level = get_keyboard_brightness(device) as f64;
    let level_discrete = (level / 51.0).round() as i32;
    let change = if up { 1 } else { -1 };
    let level_new = (level_discrete + change).clamp(0, 5) as u8 * 51;
    let _ = set_keyboard_brightness(device, level_new);
    STATE_PWR.lock().unwrap().last_key_lvl = level_new;
}

pub fn keyboard_sleep(device: &Device) {
    let _ = command(device, 0x0004, &[0, 0]); // turn off keyboard
    let _ = set_keyboard_brightness(device, 0);
    let _ = command(device, 0x0303, &[1, 38, 0]); // set VC to 0 brightness
    // let _ = command(device, 0x0300, &[1, 38, 0]); // turn off VC
}

pub fn set_mute_indicator(device: &Device, io: AudioType, muted: bool) {
    let _ = command(device, 0x1804, &[0, io as u8, muted as u8]);
}

pub fn toggle_vc(device: &Device) { // limited to 0 and 100% brightness for now
    let brightness = get_vc_brightness(device);
    let new_brightness = if brightness > 0 { 0 } else { 255 };
    let _ = command(device, 0x0303, &[1, 38, new_brightness]);
    let _ = command(device, 0x0300, &[1, 38, new_brightness / 255]);
    STATE_PWR.lock().unwrap().last_vc_active = new_brightness != 0;
}

pub fn get_vc_brightness(device: &Device) -> u8 {
    let brightness = command(device, 0x0383, &[1, 38, 0]).unwrap();
    let active = command(device, 0x0380, &[1, 38, 0]).unwrap();
    brightness * active
}

pub fn cycle_rgb_mode(device: &Device) {
    STATE_PWR.lock().unwrap().last_keyboard_rgb += 1;
    let _ = command(device, 0x030a, &[STATE_PWR.lock().unwrap().rgb_code(), 0]);
}

pub fn set_keyboard_brightness(device: &Device, brightness: u8) {
    let _ = command(device, 0x0303, &[1, 5, brightness]);
}

pub fn get_keyboard_brightness(device: &Device) -> u8 {
    command(device, 0x0383, &[1, 5, 0]).unwrap()
}

use std::sync::mpsc::{self, Sender, Receiver};
use std::thread;
use std::time::Duration;

static SENDER: OnceLock<mpsc::Sender<DeviceCmd>> = OnceLock::new();

pub enum DeviceCmd {
    InitializeKeyboard,
    KeyboardSleep,
    KeyboardLight(bool),
    GetPID(mpsc::Sender::<u16>),
    SetMuteIndicator(AudioType, bool),
    CycleRGBMode,
    ToggleVC,
    Shutdown,
}

pub struct DeviceHandle {
    sender: Sender<DeviceCmd>,
}

impl DeviceHandle {
    fn get<T, F>(&self, make_query: F) -> anyhow::Result<T>
    where
        F: FnOnce(mpsc::Sender<T>) -> DeviceCmd
    {
        let (resp_tx, resp_rx) = mpsc::channel::<T>();

        // Construct the command (e.g., DeviceCmd::GetKeyboardBrightness(resp_tx))
        let cmd = make_query(resp_tx);

        self.sender.send(cmd).expect("Device handle thread is no longer running");

        // Block until the handle sends T back
        let resp = resp_rx.recv_timeout(Duration::from_secs(1))
            .map_err(|_| anyhow::anyhow!("Hardware response timeout"))?;

        Ok(resp)
    }
    pub fn keyboard_light_up(&self) {
        let _ = self.sender.send(DeviceCmd::KeyboardLight(true));
    }
    pub fn keyboard_light_down(&self) {
        let _ = self.sender.send(DeviceCmd::KeyboardLight(false));
    }
    pub fn get_pid(&self) -> u16 {
        self.get(DeviceCmd::GetPID).expect("Device PID config error")
    }
    pub fn initialize_keyboard(&self) {
        let _ = self.sender.send(DeviceCmd::InitializeKeyboard);
    }
    pub fn keyboard_sleep(&self) {
        let _ = self.sender.send(DeviceCmd::KeyboardSleep);
    }
    pub fn set_speakers_mute_indicator(&self, muted: bool) {
        let _ = self.sender.send(DeviceCmd::SetMuteIndicator(AudioType::Speakers, muted));
    }
    pub fn set_mic_mute_indicator(&self, muted: bool) {
        let _ = self.sender.send(DeviceCmd::SetMuteIndicator(AudioType::Mic, muted));
    }
    pub fn cycle_rgb_mode(&self) {
        let _ = self.sender.send(DeviceCmd::CycleRGBMode);
    }
    pub fn toggle_vc(&self) {
        let _ = self.sender.send(DeviceCmd::ToggleVC);
    }
}

pub fn device() -> DeviceHandle {
    let tx = SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<DeviceCmd>();

        thread::spawn(move || {
            let device = Device::detect().expect("No compatible device found");
            handle_recv(&device, rx);
        });

        tx
    });

    DeviceHandle { sender: tx.clone() }
}

fn handle_recv(device: &Device, rx: Receiver<DeviceCmd>) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            DeviceCmd::KeyboardLight(up) => {
                let _ = keyboard_light(device, up);
            },
            DeviceCmd::GetPID(tx) => {
                let _ = tx.send(device.info.pid);
            },
            DeviceCmd::InitializeKeyboard => {
                let _ = initialize_keyboard(device);
            },
            DeviceCmd::KeyboardSleep => {
                let _ = keyboard_sleep(device);
            }
            DeviceCmd::SetMuteIndicator(io, muted) => {
                let _ = set_mute_indicator(device, io, muted);
            },
            DeviceCmd::CycleRGBMode => {
                let _ = cycle_rgb_mode(device);
            },
            DeviceCmd::ToggleVC => {
                let _ = toggle_vc(device);
            }
            DeviceCmd::Shutdown => break,
        }
    }
    println!("All handles dropped. Handle thread exiting.");
}