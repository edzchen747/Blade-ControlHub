use librazer::{device::Device, packet::Packet};

use crate::{
    razer::{
        device_handle::DeviceCmd,
        device_state::{AppConfig, save_config},
    },
    win::actions::AudioType,
    win::persist::PersistBuffer,
};
use std::{sync::mpsc::Receiver, thread, time::Duration};

pub struct Executer<'a> {
    pub device: &'a Device,
    pub app_config: &'a mut AppConfig,
    pub persist_buffer: PersistBuffer,
    rx: Receiver<DeviceCmd>,
}

impl<'a> Executer<'a> {
    pub fn new(
        device: &'a Device,
        app_config: &'a mut AppConfig,
        persist_buffer: PersistBuffer,
        rx: Receiver<DeviceCmd>,
    ) -> Self {
        Self {
            device,
            app_config,
            persist_buffer,
            rx,
        }
    }
    pub fn process_commands(&mut self) {
        while let Ok(cmd) = self.rx.recv() {
            match cmd {
                DeviceCmd::KeyboardLight(up) => {
                    self.keyboard_light(up);
                }
                DeviceCmd::GetPID(tx) => {
                    let _ = tx.send(self.device.info.pid);
                }
                DeviceCmd::GetPerfMode(tx) => {
                    let _ = tx.send(self.get_perf_mode());
                }
                DeviceCmd::InitializeDevice => {
                    self.initialize_device();
                }
                DeviceCmd::SleepDevice => {
                    self.sleep_device();
                }
                DeviceCmd::SetMuteIndicator(io, muted) => {
                    self.set_mute_indicator(io, muted);
                }
                DeviceCmd::CycleRGBMode => {
                    self.cycle_rgb_mode();
                }
                DeviceCmd::CyclePerfMode => {
                    self.cycle_perf_mode();
                }
                DeviceCmd::ToggleVC => {
                    self.toggle_vc();
                }
                DeviceCmd::Shutdown => break,
            }
        }
        println!("All handles dropped. Handle thread exiting.");
    }

    pub fn save_config(&self) {
        save_config(self.app_config, &self.persist_buffer);
    }

    // internal functions
    fn initialize_device(&mut self) {
        let _ = command(self.device, 0x0004, &[3, 0]); // turn on keyboard
        let _ = command(self.device, 0x0206, &[0, 1]); // set multi media keys if configured
        // let _ = command(self.device, 0x0086, &[0, 0, 0]); // set multi media keys if configured
        let rgb_effect = self.app_config.power_state.rgb_effect.value();
        let _ = command(self.device, 0x030a, &[rgb_effect, 0]); // set effect
        let _ = command(self.device, 0x0300, &[1, 38, 1]); // Turn on VC
        let _ = command(
            self.device,
            0x0303,
            &[1, 38, (self.app_config.power_state.last_vc_lvl)],
        );
        self.set_keyboard_brightness(self.app_config.power_state.last_key_lvl); // set brightness
        let curr_perf_mode = self.app_config.power_state.perf_mode.value();
        self.set_perf_mode(curr_perf_mode);
    }

    fn sleep_device(&self) {
        // do not save any values
        let _ = command(self.device, 0x0004, &[0, 0]); // turn off keyboard
        let _ = command(self.device, 0x0303, &[1, 5, 0]); // set keyboard to 0 brightness
        // VC brightness changes must be done in this order
        let _ = command(self.device, 0x0300, &[1, 38, 0]); // turn off VC
        let _ = command(self.device, 0x0303, &[1, 38, 0]); // set VC to 0 brightness
        let _ = command(self.device, 0x0d02, &[1, 0, 6, 0]); // set perf mode
    }

    fn keyboard_light(&mut self, up: bool) {
        let level = self.get_keyboard_brightness() as f64;
        let level_discrete = (level / 51.0).round() as i32;
        let change = if up { 1 } else { -1 };
        let level_new = (level_discrete + change).clamp(0, 5) as u8 * 51;
        self.set_keyboard_brightness(level_new);
        self.app_config.power_state.last_key_lvl = level_new;
        self.save_config();
    }

    fn toggle_vc(&mut self) {
        // limited to 0 and 100% brightness for now
        let brightness = self.get_vc_brightness();
        let new_brightness = if brightness > 0 { 0 } else { 255 };
        let _ = command(self.device, 0x0303, &[1, 38, new_brightness]);
        let _ = command(self.device, 0x0300, &[1, 38, new_brightness / 255]);
        self.app_config.power_state.last_vc_lvl = new_brightness;
        self.save_config();
    }

    fn cycle_rgb_mode(&mut self) {
        let new_rgb_effect = self.app_config.power_state.rgb_effect.next();
        self.set_rgb_effect(new_rgb_effect);
    }

    fn set_rgb_effect(&mut self, rgb_effect: u8) {
        let _ = command(self.device, 0x030a, &[rgb_effect, 0]);
        self.app_config.power_state.rgb_effect.set(&rgb_effect);
        self.save_config();
    }

    fn cycle_perf_mode(&mut self) {
        let new_perf_mode = self.app_config.power_state.perf_mode.next();
        self.set_perf_mode(new_perf_mode);
    }

    fn set_perf_mode(&mut self, perf_mode: u8) {
        let _ = command(self.device, 0x0d02, &[1, 0, perf_mode, 0]);
        self.app_config
            .power_state
            .perf_mode
            .set(&self.get_perf_mode());
        self.save_config();
    }

    fn set_keyboard_brightness(&mut self, brightness: u8) {
        let _ = command(self.device, 0x0303, &[1, 5, brightness]);
        self.app_config.power_state.last_key_lvl = self.get_keyboard_brightness();
        self.save_config();
    }

    fn get_keyboard_brightness(&self) -> u8 {
        command(self.device, 0x0383, &[1, 5, 0])
    }

    fn get_perf_mode(&self) -> u8 {
        let perf_mode = command(self.device, 0x0d82, &[0, 0, 0, 0]);
        let proxy = crate::GUI_PROXY
            .get()
            .expect("Fatal internal error: get gui proxy");
        proxy
            .send_event(perf_mode)
            .expect("Fatal internal error: gui proxy send");
        perf_mode
    }

    fn get_vc_brightness(&self) -> u8 {
        let brightness = command(self.device, 0x0383, &[1, 38, 0]);
        let active = command(self.device, 0x0380, &[1, 38, 0]);
        brightness * active
    }

    fn set_mute_indicator(&self, io: AudioType, muted: bool) {
        let _ = command(self.device, 0x1804, &[0, io as u8, muted as u8]);
    }
}

fn command(device: &Device, command: u16, args: &[u8]) -> u8 {
    for attempt in 1..=3 {
        let report = Packet::new(command, args);
        match device.send(report) {
            Ok(response) => {
                if response.get_args().len() >= args.len() && response_valid(&response, args) {
                    return response.get_args()[2];
                } else {
                    println!("Error: Response invalid");
                }
            }
            Err(err) => println!("{:?}, command: {:#06x} {:?}", err, command, args),
        };
        thread::sleep(Duration::from_millis(100 * attempt));
    }
    println!(
        "Command failed 3 times, skipping ({:#06x} {:?})",
        command, args
    );
    0
}

fn response_valid(response: &Packet, args: &[u8]) -> bool {
    response
        .get_args()
        .iter()
        .enumerate()
        .take(args.len())
        .filter(|&(i, _)| i != 2) // SKIP index 2
        .all(|(i, &byte)| byte == args[i])
}
