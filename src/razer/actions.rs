use crate::win::actions::AudioType;

use librazer::{command};
use librazer::device::{Device};
use anyhow::{Result, ensure};
use librazer::packet::Packet;
use std::sync::{OnceLock, Mutex, MutexGuard};

struct DeviceState {
    last_key_lvl: u8,
    last_keyboard_rgb: u32,
    last_vc_active: bool
}

impl DeviceState {
    const fn new() -> Self {
        Self {
            last_key_lvl: 0,
            last_keyboard_rgb: 0,
            last_vc_active: true
        }
    }

    pub fn rgb(&self) -> u8 {
        RGB_VALS[(self.last_keyboard_rgb % RGB_VALS.len() as u32) as usize]
    }
}

static RGB_VALS: [u8; 3] = [4, 1, 3];

static DEVICE: OnceLock<Mutex<Device>> = OnceLock::new();
static STATE_PWR: Mutex<DeviceState> = Mutex::new(DeviceState::new());
static STATE_BATT: Mutex<DeviceState> = Mutex::new(DeviceState::new());

pub fn init_device() -> Result<(), anyhow::Error> {
    match Device::detect() {
        Ok(d) => DEVICE.set(Mutex::new(d)).map_err(|_| anyhow::anyhow!("Failed to initilize device")),
        Err(e) => Err(e)
    }
}

pub fn device() -> MutexGuard<'static, Device> {
    DEVICE
        .get()
        .expect("Device not yet initialized")
        .lock()
        .expect("Device initialization failed")
}

fn with_device<F>(f: F) -> anyhow::Result<()>
where
    F: FnOnce(&Device),
{
    let guard = device();
    f(&*guard);
    Ok(())
}

pub fn custom_command(device: &Device, command: u16, args: &[u8]) -> Result<u8> {
    for _attempt in 1..=3 {
        let report = Packet::new(command, args);
        let response = device.send(report)?;
        if response.get_args().starts_with(args) {
            return Ok(response.get_args()[2]);
        }
    }
    Ok(0)
}

pub fn init_keyboard_state(save_state: bool) -> Result<()> {
    with_device(|dev| {
        let _ = custom_command(dev, 0x030a, &[4]);
        let _ = custom_command(dev, 0x0004, &[3, 0]);
        let _ = custom_command(dev, 0x0206, &[0, 1]);
        let _ = custom_command(dev, 0x0086, &[0, 0, 0]);
        let _ = custom_command(dev, 0x030a, &[STATE_PWR.lock().unwrap().rgb(), 0]);
        // let _ = custom_command(dev, 0x0300, &[1, 38, STATE_PWR.lock().unwrap().last_vc_active as u8]);
        let _ = custom_command(dev, 0x0303, &[1, 38, (STATE_PWR.lock().unwrap().last_vc_active as u8) * 255]);
        if save_state {
            STATE_PWR.lock().unwrap().last_key_lvl = command::get_keyboard_brightness(dev).unwrap();
        } else {
            let _ = command::set_keyboard_brightness(dev, STATE_PWR.lock().unwrap().last_key_lvl);
        };
    })
}

pub fn keyboard_light(up: bool) -> Result<()> {
    with_device(|dev| {
        let level = command::get_keyboard_brightness(&dev).unwrap() as f64;
        let level_discrete = (level / 51.0).round() as i32;
        let change = if up { 1 } else { -1 };
        let level_new = (level_discrete + change).clamp(0, 5) as u8 * 51;
        let _ = command::set_keyboard_brightness(&dev, level_new);
        STATE_PWR.lock().unwrap().last_key_lvl = level_new;
    })
}

pub fn keyboard_sleep() -> Result<()> {
    with_device(|dev| {
        let _ = custom_command(dev, 0x0004, &[0, 0]);
        let _ = command::set_keyboard_brightness(dev, 0);
        let _ = custom_command(dev, 0x0303, &[1, 38, 0]);
        // let _ = custom_command(dev, 0x0300, &[1, 38, 0]);
    })
}

pub fn set_mute_indicator(io: AudioType, muted: bool) -> Result<()> {
    with_device(|dev| {
        let _ = custom_command(dev, 0x1804, &[0, io as u8, muted as u8]);
    })
}

pub fn toggle_vc_brightness() -> Result<()> {
    with_device(|dev| {
        let brightness = get_vc_brightness(dev).unwrap();
        let new_brightness = if brightness > 0 { 0 } else { 255 };
        let _ = custom_command(dev, 0x0303, &[1, 38, new_brightness]);
        let _ = custom_command(dev, 0x0300, &[1, 38, new_brightness / 255]);
        STATE_PWR.lock().unwrap().last_vc_active = new_brightness != 0;
    })
}

pub fn get_vc_brightness(device: &Device,) -> Result<u8> {
    let brightness = device.send(Packet::new(0x0383, &[1, 38, 0]))?;
    ensure!(brightness.get_args()[1] == 38);
    let active = device.send(Packet::new(0x0380, &[1, 38, 0]))?;
    ensure!(active.get_args()[1] == 38);
    Ok(brightness.get_args()[2] * active.get_args()[2])
}

pub fn cycle_rgb_mode() -> Result<()> {
    with_device(|dev| {
        STATE_PWR.lock().unwrap().last_keyboard_rgb += 1;
        let _ = custom_command(dev, 0x030a, &[STATE_PWR.lock().unwrap().rgb(), 0]);
    })
}

// pub fn set_keyboard_brightness(device: &Device, brightness: u8) -> Result<()> {
//     let _ = custom_command(device, 0x0303, &[1, 5, brightness]);
//     Ok(())
// }

// pub fn get_keyboard_brightness(device:) -> Result<u8> {
//     custom_command(dev, 0x0383, &[1, 5, 0])
// }