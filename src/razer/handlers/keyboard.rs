use librazer::{command::custom_command, device::Device};

use crate::{
    config::persist_config,
    core::shared_state::DEFAULT_MULTIMEDIA_KEYS,
    razer::{
        config::AppConfig,
        device_handle::device,
        enums::{LidLogoMode, RGBEffect},
        protocol::command,
    },
    ui::{app::app, app_events::OsdEvent},
    utils::persist::PersistBuffer,
    win::display::ambient::AmbientEffect,
};
use std::sync::atomic::Ordering;

/// Keyboard, RGB, lighting, and key-mode handler.
///
/// Accepts references to the device, app config, and persist buffer.
/// All logic is copied exactly from Executer — zero-cost abstraction.
pub struct KeyboardHandler<'a> {
    device: &'a Device,
    app_config: &'a mut AppConfig,
    persist_buffer: &'a PersistBuffer,
}

impl<'a> KeyboardHandler<'a> {
    pub fn new(
        device: &'a Device,
        app_config: &'a mut AppConfig,
        persist_buffer: &'a PersistBuffer,
    ) -> Self {
        Self {
            device,
            app_config,
            persist_buffer,
        }
    }

    // ── Keyboard ────────────────────────────────────────────────────────

    pub fn adjust_keyboard_light(&mut self, up: bool) {
        let level = self.get_keyboard_brightness() as f64;
        let level_discrete = (level / 51.0).round() as i32;
        let change = if up { 1 } else { -1 };
        let level_new = (level_discrete + change).clamp(0, 5) as u8 * 51;
        self.set_keyboard_brightness(level_new);
        self.app_config.get().key_lvl = level_new;
        self.persist_config();
    }

    pub fn set_keyboard_brightness(&mut self, brightness: u8) {
        let _ = command(self.device, 0x0303, &[1, 5, brightness], None);
        let key_lvl = self.get_keyboard_brightness();
        self.app_config.get().key_lvl = key_lvl;
        self.persist_config();
    }

    pub fn get_keyboard_brightness(&self) -> u8 {
        // unwrap_or(0): returns 0 on hardware/protocol failure
        let brightness = command(self.device, 0x0383, &[1, 5, 0], Some(2)).unwrap_or(0);
        app().send(OsdEvent::KeyboardBrightness(brightness).into());
        brightness
    }

    pub fn get_default_multimedia_keys(&self) -> bool {
        let current_default = self.app_config.default_multimedia_keys;
        DEFAULT_MULTIMEDIA_KEYS.store(current_default, Ordering::SeqCst);
        current_default
    }

    pub fn toggle_default_multimedia_keys(&mut self) -> bool {
        let current_default = self.app_config.default_multimedia_keys;
        self.app_config.default_multimedia_keys = !current_default;
        match !current_default {
            true => self.enable_multimedia_keys(),
            false => self.restore_fn_keys(),
        }
        self.persist_config();
        self.get_default_multimedia_keys()
    }

    // ── RGB & Lighting ──────────────────────────────────────────────────

    pub fn cycle_rgb_mode(&mut self) {
        let new_rgb_effect = self.app_config.get().rgb_effect.next();
        self.set_rgb_effect(new_rgb_effect);
    }

    pub fn set_rgb_effect(&mut self, rgb_effect: RGBEffect) {
        if rgb_effect == RGBEffect::Ambient {
            AmbientEffect::start(device());
            app().send(OsdEvent::RGBEffect(rgb_effect).into());
        } else {
            AmbientEffect::stop();
        }

        let mut args = vec![rgb_effect as u8, 0];
        if rgb_effect == RGBEffect::Reactive {
            args = vec![rgb_effect as u8, 0, 32, 255, 255, 255];
        }
        let _ = command(self.device, 0x030a, &args, None);

        // value comes from hardware readback; mismatch is non-fatal
        let _ = self.app_config.get().rgb_effect.set(&rgb_effect);
        if rgb_effect != RGBEffect::Ambient {
            let effect = self.get_rgb_effect();
            app().send(OsdEvent::RGBEffect(effect).into());
        }
        self.persist_config();
    }

    pub fn get_rgb_effect(&self) -> RGBEffect {
        // unwrap_or(0): returns RGBEffect 0 on hardware/protocol failure
        command(self.device, 0x038a, &[0], Some(0))
            .unwrap_or(0)
            .into()
    }

    pub fn toggle_under_glow(&mut self) {
        let brightness = self.get_under_glow_brightness();
        let new_brightness = if brightness > 0 { 0 } else { 255 };
        let _ = command(self.device, 0x0303, &[1, 38, new_brightness], None);
        let _ = command(self.device, 0x0300, &[1, 38, new_brightness / 255], None);
        app().send(OsdEvent::UnderGlow(new_brightness).into());
        self.app_config.get().vc_lvl = new_brightness;
        self.persist_config();
    }

    pub fn enable_under_glow(&self, brightness: u8) {
        let _ = command(self.device, 0x0300, &[1, 38, 1], None);
        let _ = command(self.device, 0x0303, &[1, 38, brightness], None);
    }

    pub fn get_under_glow_brightness(&self) -> u8 {
        // unwrap_or(0): returns 0 on hardware/protocol failure
        let brightness = command(self.device, 0x0383, &[1, 38, 0], Some(2)).unwrap_or(0);
        let active = command(self.device, 0x0380, &[1, 38, 0], Some(2)).unwrap_or(0);
        brightness * active
    }

    pub fn set_keyboard_color(&self, r: u8, g: u8, b: u8) {
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

    pub fn set_lid_logo(&mut self, mode: LidLogoMode) {
        if mode == LidLogoMode::Off {
            let _ = command(self.device, 0x0300, &[1, 4, 0], None);
        } else {
            let _ = command(self.device, 0x0300, &[1, 4, 1], None);
            if mode == LidLogoMode::On {
                let _ = command(self.device, 0x0302, &[1, 4, 0], None);
            } else {
                let _ = command(self.device, 0x0302, &[1, 4, 1], None);
            }
        }
        app().send(OsdEvent::LidLogo(mode).into());
        self.persist_config();
    }

    // ── Key Modes ───────────────────────────────────────────────────────

    pub fn keyboard_control(&self, state: bool) {
        // Use Chroma-RGB, prevents Dynamic Lighting interfering
        let _ = command(self.device, 0x0f10, &[1], None);
        let arg = if state { 3 } else { 0 };
        let _ = command(self.device, 0x0004, &[arg, 0], None);
    }

    pub fn enable_multimedia_keys(&self) {
        let _ = command(self.device, 0x0206, &[0, 1], None);
    }

    pub fn restore_fn_keys(&self) {
        let _ = command(self.device, 0x0206, &[0, 0], None);
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn persist_config(&mut self) {
        persist_config(self.app_config, self.persist_buffer);
    }
}
