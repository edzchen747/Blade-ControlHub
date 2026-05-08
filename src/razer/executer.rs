use librazer::{command::custom_command, device::Device, packet::Packet};

use crate::{
    razer::{
        device_handle::{DeviceCmd, device},
        device_state::{AppConfig, PerfMode, RGBEffect, persist_config},
    },
    ui::{app_events::AppEvent, tray_app::tray_app},
    win::{
        actions::AudioType, ambient_effect::AmbientEffect, brightness::BrightnessWorker,
        display::DisplayManager, persist::PersistBuffer,
    },
};
use std::time::Instant;
use std::{sync::mpsc::Receiver, thread, time::Duration};

pub struct Executer<'a> {
    pub device: &'a Device,
    pub app_config: &'a mut AppConfig,
    pub persist_buffer: PersistBuffer,
    rx: Receiver<DeviceCmd>,
    brightness_worker: BrightnessWorker,
    display_manager: DisplayManager,
    refresh_cycle_timeout: Instant,
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
            brightness_worker: BrightnessWorker::new(),
            display_manager: DisplayManager::new().expect("Error: No displays found"),
            refresh_cycle_timeout: Instant::now(),
        }
    }
    pub fn process_commands(&mut self) {
        while let Ok(cmd) = self.rx.recv() {
            match cmd {
                DeviceCmd::AdjustKeyboardLight(up) => {
                    self.adjust_keyboard_light(up);
                }
                DeviceCmd::GetPID(tx) => {
                    let _ = tx.send(self.device.info.pid);
                }
                DeviceCmd::GetPerfMode(tx) => {
                    let _ = tx.send(self.get_perf_mode());
                }
                DeviceCmd::InitializeDevice => {
                    self.initialize();
                }
                DeviceCmd::SleepDevice => {
                    self.sleep();
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
                DeviceCmd::ToggleUnderGlow => {
                    self.toggle_under_glow();
                }
                DeviceCmd::AdjustScreenBrightness(change) => {
                    self.brightness_worker.adjust_screen_brightness(change);
                }
                DeviceCmd::SetScreenBrightness(value) => {
                    self.brightness_worker
                        .set_screen_brightness(value.clamp(0, 100));
                }
                DeviceCmd::GetRefreshRate(tx) => {
                    let _ = tx.send(self.get_refresh_rate());
                }
                DeviceCmd::CycleRefreshRate => {
                    self.cycle_refresh_rate();
                }
                DeviceCmd::KeyboardColor(r, g, b) => {
                    self.keyboard_color(r, g, b);
                }
                DeviceCmd::PersistConfig => {
                    self.persist_config();
                }
                DeviceCmd::Shutdown => {
                    self.shutdown();
                }
            }
        }
        println!("All handles dropped. Handle thread exiting.");
    }

    pub fn persist_config(&mut self) {
        persist_config(self.app_config, &self.persist_buffer);
    }

    // internal functions
    fn initialize(&mut self) {
        // Must set screen brightness first so SCREEN_TARGET_LVL is updated before next config persist
        let screen_lvl = self.app_config.read().screen_lvl;
        self.brightness_worker.set_screen_brightness(screen_lvl);
        self.multi_media_keys();
        // let _ = command(self.device, 0x0086, &[0, 0, 0], None); // set multi media keys if configured
        let rgb_effect = self.app_config.read().rgb_effect.value();
        self.set_rgb_effect(rgb_effect, false);
        let _ = command(self.device, 0x0300, &[1, 38, 1], None); // Turn on Under Glow
        let vc_active = self.app_config.read().vc_lvl;
        let _ = command(self.device, 0x0303, &[1, 38, vc_active], None);
        self.set_keyboard_brightness(self.app_config.read().key_lvl); // set brightness
        let curr_perf_mode = self.app_config.read().perf_mode.value();
        self.set_perf_mode(curr_perf_mode);
        let refresh_rate = self.app_config.read().screen_refresh;
        if refresh_rate != 0 {
            self.set_refresh_rate(refresh_rate);
        }
    }

    fn sleep(&self) {
        // do not save any values
        let _ = command(self.device, 0x0004, &[0, 0], None); // reset to default state
        let _ = command(self.device, 0x030a, &[5, 0], None); // reset to blank RGB effect
        AmbientEffect::stop();
        let _ = command(self.device, 0x0303, &[1, 5, 0], None); // set keyboard to 0 brightness
        // Under Glow brightness changes must be done in this order turn off then set brightness to 0
        let _ = command(self.device, 0x0300, &[1, 38, 0], None); // turn off Under Glow
        let _ = command(self.device, 0x0303, &[1, 38, 0], None); // set Under Glow to 0 brightness
        let _ = command(self.device, 0x0d02, &[1, 0, 6, 0], None); // set perf mode
    }

    fn adjust_keyboard_light(&mut self, up: bool) {
        let level = self.get_keyboard_brightness() as f64;
        let level_discrete = (level / 51.0).round() as i32;
        let change = if up { 1 } else { -1 };
        let level_new = (level_discrete + change).clamp(0, 5) as u8 * 51;
        self.set_keyboard_brightness(level_new);
        self.app_config.get().key_lvl = level_new;
        self.persist_config();
    }

    fn toggle_under_glow(&mut self) {
        // limited to 0 and 100% brightness for now
        let brightness = self.get_under_glow_brightness();
        let new_brightness = if brightness > 0 { 0 } else { 255 };
        let _ = command(self.device, 0x0303, &[1, 38, new_brightness], None);
        let _ = command(self.device, 0x0300, &[1, 38, new_brightness / 255], None);
        tray_app().send(AppEvent::UnderGlow(new_brightness));
        self.app_config.get().vc_lvl = new_brightness;
        self.persist_config();
    }

    fn cycle_rgb_mode(&mut self) {
        let new_rgb_effect = self.app_config.get().rgb_effect.next();
        self.set_rgb_effect(new_rgb_effect, true);
    }

    fn set_rgb_effect(&mut self, rgb_effect: RGBEffect, save: bool) {
        if rgb_effect == RGBEffect::Ambient {
            AmbientEffect::start(device());
            tray_app().send(AppEvent::RGBEffect(rgb_effect));
        } else {
            AmbientEffect::stop();
        }
        let _ = command(self.device, 0x030a, &[rgb_effect as u8, 0], None);
        if save {
            self.app_config.get().rgb_effect.set(&rgb_effect);
            if rgb_effect != RGBEffect::Ambient {
                let effect = self.get_rgb_effect();
                tray_app().send(AppEvent::RGBEffect(effect));
            }
            self.persist_config();
        }
    }

    fn get_rgb_effect(&self) -> RGBEffect {
        command(self.device, 0x038a, &[0, 3], Some(0)).into()
    }

    fn cycle_perf_mode(&mut self) {
        let new_perf_mode = self.app_config.get().perf_mode.next();
        self.set_perf_mode(new_perf_mode);
    }

    fn set_perf_mode(&mut self, perf_mode: PerfMode) {
        println!("Set Perf Mode: {:?}", perf_mode.to_string());
        let _ = command(self.device, 0x0d02, &[1, 0, perf_mode as u8, 0], None);
        let perf_mode = self.get_perf_mode();
        self.app_config.get().perf_mode.set(&perf_mode);
        self.persist_config();
    }

    fn set_keyboard_brightness(&mut self, brightness: u8) {
        let _ = command(self.device, 0x0303, &[1, 5, brightness], None);
        let key_lvl = self.get_keyboard_brightness();
        self.app_config.get().key_lvl = key_lvl;
        self.persist_config();
    }

    fn get_keyboard_brightness(&self) -> u8 {
        let brightness = command(self.device, 0x0383, &[1, 5, 0], Some(2));
        tray_app().send(AppEvent::KeyboardBrightness(brightness));
        brightness
    }

    fn get_perf_mode(&self) -> PerfMode {
        let perf_mode: PerfMode = command(self.device, 0x0d82, &[0, 0, 0, 0], Some(2)).into();
        tray_app().send(AppEvent::PerfMode(perf_mode));
        perf_mode
    }

    fn get_under_glow_brightness(&self) -> u8 {
        let brightness = command(self.device, 0x0383, &[1, 38, 0], Some(2));
        let active = command(self.device, 0x0380, &[1, 38, 0], Some(2));
        brightness * active
    }

    fn set_mute_indicator(&self, io: AudioType, muted: bool) {
        let _ = command(self.device, 0x1804, &[0, io as u8, muted as u8], None);
    }

    fn multi_media_keys(&self) {
        let _ = command(self.device, 0x0004, &[3, 0], None);
        let _ = command(self.device, 0x0206, &[0, 1], None);
    }

    fn multi_fn_keys(&self) {
        let _ = command(self.device, 0x0004, &[0, 0], None);
        let _ = command(self.device, 0x0206, &[0, 0], None);
    }

    fn get_refresh_rate(&mut self) -> u32 {
        let current = self.display_manager.get_current_rate();
        let supported = self.display_manager.get_supported_rates();
        let level = 1 + supported
            .iter()
            .position(|&rate| rate == current)
            .unwrap_or(0);
        tray_app().send(AppEvent::RefreshRate(
            current,
            level as u8,
            supported.len() as u8,
        ));
        self.app_config.get().screen_refresh = current;
        self.persist_config();
        self.refresh_cycle_timeout = Instant::now() + Duration::from_millis(1500);
        current
    }

    fn cycle_refresh_rate(&mut self) {
        if Instant::now() < self.refresh_cycle_timeout {
            let _ = self.display_manager.cycle_refresh_rate();
        }
        self.get_refresh_rate();
    }

    fn set_refresh_rate(&mut self, refresh_rate: u32) {
        if self
            .display_manager
            .get_supported_rates()
            .contains(&refresh_rate)
        {
            if self.display_manager.get_current_rate() != refresh_rate {
                let _ = self.display_manager.set_refresh_rate(refresh_rate);
                self.get_refresh_rate();
            }
        }
    }

    fn keyboard_color(&self, r: u8, g: u8, b: u8) {
        let mut args = vec![
            255, 0, 0, 18, 0, 0, 0, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b,
            r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g,
            b, r, g, b,
        ];
        for row in 0..=6 {
            args[1] = row;
            let _ = custom_command(self.device, 0x030b, &args);
        }
    }

    fn shutdown(&self) {
        self.multi_fn_keys();
    }
}

fn command(device: &Device, command: u16, args: &[u8], result_idx: Option<usize>) -> u8 {
    for attempt in 1..=3 {
        let report = Packet::new(command, args);
        match device.send(report) {
            Ok(response) => {
                if response.get_args().len() >= args.len()
                    && response_valid(&response, args, result_idx)
                {
                    if let Some(idx) = result_idx {
                        return response.get_args()[idx];
                    } else {
                        return 0;
                    }
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

fn response_valid(response: &Packet, args: &[u8], idx: Option<usize>) -> bool {
    if idx.is_none() {
        return true;
    }
    response
        .get_args()
        .iter()
        .enumerate()
        .take(args.len())
        .filter(|&(i, _)| i != idx.unwrap()) // SKIP index 2
        .all(|(i, &byte)| byte == args[i])
}
