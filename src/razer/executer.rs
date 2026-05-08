use librazer::{command::custom_command, device::Device};

use crate::{
    config::persist_config,
    razer::{
        config::AppConfig,
        device_handle::{DeviceCmd, device},
        enums::{PerfMode, RGBEffect},
        protocol::command,
    },
    ui::{app_events::AppEvent, tray_app::tray_app},
    utils::persist::PersistBuffer,
    win::{
        audio::AudioType,
        display::{
            ambient::AmbientEffect, brightness::BrightnessWorker, refresh_rate::DisplayManager,
        },
    },
};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

// ── Executer ────────────────────────────────────────────────────────────────

/// Processes device commands received from `DeviceHandle` on a dedicated thread.
/// Owns all mutable device state and orchestrates hardware interactions.
pub struct Executer<'a> {
    device: &'a Device,
    app_config: &'a mut AppConfig,
    persist_buffer: PersistBuffer,
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

    // ── Command Loop ────────────────────────────────────────────────────

    /// Main event loop: blocks on the receiver and dispatches each command.
    pub fn process_commands(&mut self) {
        while let Ok(cmd) = self.rx.recv() {
            self.dispatch(cmd);
        }
        println!("All handles dropped. Handle thread exiting.");
    }

    fn dispatch(&mut self, cmd: DeviceCmd) {
        match cmd {
            DeviceCmd::InitializeDevice => self.initialize(),
            DeviceCmd::SleepDevice => self.sleep(),
            DeviceCmd::Shutdown => self.shutdown(),

            // Keyboard
            DeviceCmd::AdjustKeyboardLight(up) => self.adjust_keyboard_light(up),
            DeviceCmd::KeyboardColor(r, g, b) => self.keyboard_color(r, g, b),

            // RGB & lighting
            DeviceCmd::CycleRGBMode => self.cycle_rgb_mode(),
            DeviceCmd::ToggleUnderGlow => self.toggle_under_glow(),

            // Performance
            DeviceCmd::CyclePerfMode => self.cycle_perf_mode(),

            // Display
            DeviceCmd::AdjustScreenBrightness(change) => {
                self.brightness_worker.adjust_screen_brightness(change);
            }
            DeviceCmd::SetScreenBrightness(value) => {
                self.brightness_worker
                    .set_screen_brightness(value.clamp(0, 100));
            }
            DeviceCmd::CycleRefreshRate => self.cycle_refresh_rate(),

            // Audio
            DeviceCmd::SetMuteIndicator(io, muted) => self.set_mute_indicator(io, muted),

            // Queries
            DeviceCmd::GetPID(tx) => {
                let _ = tx.send(self.device.info.pid);
            }
            DeviceCmd::GetPerfMode(tx) => {
                let _ = tx.send(self.get_perf_mode());
            }
            DeviceCmd::GetRefreshRate(tx) => {
                let _ = tx.send(self.get_refresh_rate());
            }

            // Config
            DeviceCmd::PersistConfig => self.persist_config(),
        }
    }

    // ── Initialization & Lifecycle ──────────────────────────────────────

    fn initialize(&mut self) {
        // Must set screen brightness first so SCREEN_TARGET_LVL is updated before next config persist
        let mut state = self.app_config.read();

        self.brightness_worker
            .set_screen_brightness(state.screen_lvl);
        self.enable_multimedia_keys();
        self.set_rgb_effect(state.rgb_effect.value(), false);
        self.enable_under_glow(state.vc_lvl);
        self.set_keyboard_brightness(state.key_lvl);
        self.set_perf_mode(state.perf_mode.value());

        if state.screen_refresh != 0 {
            self.set_refresh_rate(state.screen_refresh);
        }
    }

    fn sleep(&self) {
        let _ = command(self.device, 0x0004, &[0, 0], None); // reset to default state
        let _ = command(self.device, 0x030a, &[5, 0], None); // blank RGB effect
        AmbientEffect::stop();
        let _ = command(self.device, 0x0303, &[1, 5, 0], None); // keyboard brightness 0
        // Under Glow: must turn off first, then set brightness to 0
        let _ = command(self.device, 0x0300, &[1, 38, 0], None); // turn off
        let _ = command(self.device, 0x0303, &[1, 38, 0], None); // brightness 0
        let _ = command(self.device, 0x0d02, &[1, 0, 6, 0], None); // reset perf mode
    }

    fn shutdown(&self) {
        self.restore_fn_keys();
    }

    // ── Keyboard ────────────────────────────────────────────────────────

    fn adjust_keyboard_light(&mut self, up: bool) {
        let level = self.get_keyboard_brightness() as f64;
        let level_discrete = (level / 51.0).round() as i32;
        let change = if up { 1 } else { -1 };
        let level_new = (level_discrete + change).clamp(0, 5) as u8 * 51;
        self.set_keyboard_brightness(level_new);
        self.app_config.get().key_lvl = level_new;
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

    // ── RGB & Lighting ──────────────────────────────────────────────────

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

    fn toggle_under_glow(&mut self) {
        let brightness = self.get_under_glow_brightness();
        let new_brightness = if brightness > 0 { 0 } else { 255 };
        let _ = command(self.device, 0x0303, &[1, 38, new_brightness], None);
        let _ = command(self.device, 0x0300, &[1, 38, new_brightness / 255], None);
        tray_app().send(AppEvent::UnderGlow(new_brightness));
        self.app_config.get().vc_lvl = new_brightness;
        self.persist_config();
    }

    fn enable_under_glow(&self, brightness: u8) {
        let _ = command(self.device, 0x0300, &[1, 38, 1], None);
        let _ = command(self.device, 0x0303, &[1, 38, brightness], None);
    }

    fn get_under_glow_brightness(&self) -> u8 {
        let brightness = command(self.device, 0x0383, &[1, 38, 0], Some(2));
        let active = command(self.device, 0x0380, &[1, 38, 0], Some(2));
        brightness * active
    }

    // ── Performance ─────────────────────────────────────────────────────

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

    fn get_perf_mode(&self) -> PerfMode {
        let perf_mode: PerfMode = command(self.device, 0x0d82, &[0, 0, 0, 0], Some(2)).into();
        tray_app().send(AppEvent::PerfMode(perf_mode));
        perf_mode
    }

    // ── Display ─────────────────────────────────────────────────────────

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
        let supported = self.display_manager.get_supported_rates();
        if supported.contains(&refresh_rate)
            && self.display_manager.get_current_rate() != refresh_rate
        {
            let _ = self.display_manager.set_refresh_rate(refresh_rate);
            self.get_refresh_rate();
        }
    }

    // ── Audio ───────────────────────────────────────────────────────────

    fn set_mute_indicator(&self, io: AudioType, muted: bool) {
        let _ = command(self.device, 0x1804, &[0, io as u8, muted as u8], None);
    }

    // ── Key Modes ───────────────────────────────────────────────────────

    fn enable_multimedia_keys(&self) {
        let _ = command(self.device, 0x0004, &[3, 0], None);
        let _ = command(self.device, 0x0206, &[0, 1], None);
    }

    fn restore_fn_keys(&self) {
        let _ = command(self.device, 0x0004, &[0, 0], None);
        let _ = command(self.device, 0x0206, &[0, 0], None);
    }

    // ── Config ──────────────────────────────────────────────────────────

    fn persist_config(&mut self) {
        persist_config(self.app_config, &self.persist_buffer);
    }
}
