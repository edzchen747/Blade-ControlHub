use crate::config::ThemeColor;
use crate::config::persist_config;
use crate::disable_osd;
use crate::error::AppError;
use crate::razer::config::{AppConfig, PowerProfile};
use crate::razer::device_handle::DeviceCmd;
use crate::razer::enums::{BatteryLimit, PerfMode, RGBEffect};
use crate::razer::handlers::{
    AudioHandler, BatteryHandler, DisplayHandler, KeyboardHandler, PerformanceHandler,
};
use crate::razer::protocol::command;
use crate::ui::app::app;
use crate::ui::app_events::OsdEvent;
use crate::utils::persist::PersistBuffer;
use crate::win::display::ambient::AmbientEffect;
use crate::win::display::brightness::BrightnessWorker;
use crate::win::display::refresh_rate::DisplayManager;
use librazer::device::Device;
use std::sync::mpsc::Receiver;
use std::time::Instant;
use tracing::{debug, info, instrument, warn};

pub struct Executer<'a> {
    device: &'a Device,
    app_config: &'a mut AppConfig,
    persist_buffer: PersistBuffer,
    rx: Receiver<DeviceCmd>,
    brightness_worker: BrightnessWorker,
    display_manager: DisplayManager,
    refresh_cycle_timeout: Instant,
    battery_cycle_timeout: Instant,
}

impl<'a> Executer<'a> {
    pub fn new(
        device: &'a Device,
        app_config: &'a mut AppConfig,
        persist_buffer: PersistBuffer,
        rx: Receiver<DeviceCmd>,
    ) -> Result<Self, AppError> {
        let display_manager = DisplayManager::new().ok_or(AppError::DisplayNotFound)?;
        crate::ui::theme::set_runtime_theme_color(app_config.theme_color);
        Ok(Self {
            device,
            app_config,
            persist_buffer,
            rx,
            brightness_worker: BrightnessWorker::new(),
            display_manager,
            refresh_cycle_timeout: Instant::now(),
            battery_cycle_timeout: Instant::now(),
        })
    }
    pub fn process_commands(&mut self) {
        while let Ok(cmd) = self.rx.recv() {
            if !self.dispatch(cmd) {
                break;
            }
        }
        info!("All DeviceHandle senders dropped; device worker thread exiting");
    }
    fn kb(&mut self) -> KeyboardHandler<'_> {
        KeyboardHandler::new(self.device, self.app_config, &self.persist_buffer)
    }
    fn perf(&mut self) -> PerformanceHandler<'_> {
        PerformanceHandler::new(self.device, self.app_config, &self.persist_buffer)
    }
    fn display(&mut self) -> DisplayHandler<'_> {
        DisplayHandler::new(
            self.device,
            self.app_config,
            &self.persist_buffer,
            &mut self.display_manager,
            &mut self.refresh_cycle_timeout,
        )
    }
    fn battery(&mut self) -> BatteryHandler<'_> {
        BatteryHandler::new(
            self.device,
            self.app_config,
            &self.persist_buffer,
            &mut self.battery_cycle_timeout,
        )
    }
    fn dispatch(&mut self, cmd: DeviceCmd) -> bool {
        match cmd {
            DeviceCmd::InitializeDevice(notif) => self.initialize(notif),
            DeviceCmd::SleepDevice(tx) => {
                let _ = tx.send(self.sleep());
            }
            DeviceCmd::Shutdown(tx) => {
                let _ = tx.send(self.shutdown());
                return false;
            }
            DeviceCmd::AdjustKeyboardLight(up) => self.kb().adjust_keyboard_light(up),
            DeviceCmd::CycleRGBMode => self.kb().cycle_rgb_mode(),
            DeviceCmd::ToggleUnderGlow => self.kb().toggle_under_glow(),
            DeviceCmd::SetUnderGlow(profile, enabled, tx) => {
                let _ = tx.send(self.set_under_glow_for_profile(profile, enabled));
            }
            DeviceCmd::SetKeyboardColor(r, g, b, brightness) => {
                self.kb().set_keyboard_color(r, g, b, brightness)
            }
            DeviceCmd::SetKeyboardBrightness(profile, brightness, tx) => {
                let _ = tx.send(self.set_keyboard_brightness_for_profile(profile, brightness));
            }
            DeviceCmd::SetLidLogo(mode) => self.kb().set_lid_logo(mode),
            DeviceCmd::CyclePerfMode => self.perf().cycle_perf_mode(),
            DeviceCmd::SetPerfMode(profile, mode, tx) => {
                let _ = tx.send(self.set_perf_mode_for_profile(profile, mode));
            }
            DeviceCmd::AdjustScreenBrightness(change) => {
                self.brightness_worker.adjust_screen_brightness(change);
                self.persist_config();
            }
            DeviceCmd::CycleRefreshRate => self.display().cycle_refresh_rate(),
            DeviceCmd::SetRefreshRate(profile, refresh_rate, tx) => {
                let _ = tx.send(self.set_refresh_rate_for_profile(profile, refresh_rate));
            }
            DeviceCmd::DisplayLayoutChanged => self.display_layout_changed(),
            DeviceCmd::SetMuteIndicator(io, muted) => {
                AudioHandler::new(self.device).set_mute_indicator(io, muted);
            }
            DeviceCmd::CycleBatteryLimit => self.battery().cycle_battery_limit(),
            DeviceCmd::SetBatteryLimit(limit, tx) => {
                let _ = tx.send(self.set_battery_limit(limit));
            }
            DeviceCmd::SetThemeColor(color, tx) => {
                let _ = tx.send(self.set_theme_color(color));
            }
            DeviceCmd::SetRGBMode(profile, effect, tx) => {
                let _ = tx.send(self.set_rgb_effect_for_profile(profile, effect));
            }
            DeviceCmd::GetPID(tx) => {
                let _ = tx.send(self.device.info.pid);
            }
            DeviceCmd::GetModelName(tx) => {
                let _ = tx.send(self.device.info.name.to_string());
            }
            DeviceCmd::GetPerfMode(tx) => {
                let _ = tx.send(self.perf().get_perf_mode());
            }
            DeviceCmd::GetDefaultMultimediaKeys(tx) => {
                let _ = tx.send(self.kb().get_default_multimedia_keys());
            }
            DeviceCmd::ToggleDefaultMultimediaKeys(tx) => {
                let _ = tx.send(self.kb().toggle_default_multimedia_keys());
            }
            DeviceCmd::SetDefaultMultimediaKeys(enabled, tx) => {
                let _ = tx.send(self.set_default_multimedia_keys(enabled));
            }
            DeviceCmd::GetConfig(tx) => {
                let _ = tx.send(self.app_config.clone());
            }
            DeviceCmd::GetSettingsState(tx) => {
                let started = Instant::now();
                debug!("Building settings state snapshot");
                let rates_started = Instant::now();
                let supported_refresh_rates = self.display_manager.get_supported_rates();
                debug!(
                    elapsed_ms = rates_started.elapsed().as_millis() as u64,
                    rate_count = supported_refresh_rates.len(),
                    "Enumerated supported refresh rates for settings state"
                );
                let state = crate::runtime::settings_state::SettingsState::from_config(
                    self.app_config.clone(),
                    supported_refresh_rates,
                );
                info!(
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "Built settings state snapshot"
                );
                let _ = tx.send(state);
            }
            DeviceCmd::PersistConfig => {
                crate::config::persist_config(self.app_config, &self.persist_buffer);
            }
        }
        true
    }

    fn set_perf_mode_for_profile(
        &mut self,
        profile: PowerProfile,
        mode: PerfMode,
    ) -> crate::error::AppResult<()> {
        if !self
            .app_config
            .profile(profile)
            .perf_mode
            .items
            .contains(&mode)
        {
            return Err(crate::error::AppError::Internal(format!(
                "Unsupported performance mode for {profile:?}: {mode}"
            )));
        }

        if self.profile_is_active(profile) {
            if let Err(error) = self.perf().set_perf_mode(mode) {
                self.perf().remove_perf_mode(mode);
                return Err(error);
            }
        } else {
            self.app_config.profile_mut(profile).perf_mode.set(&mode)?;
            self.persist_config();
        }
        Ok(())
    }

    fn set_refresh_rate_for_profile(
        &mut self,
        profile: PowerProfile,
        refresh_rate: u32,
    ) -> crate::error::AppResult<()> {
        let supported = self.display_manager.get_supported_rates();
        if !supported.contains(&refresh_rate) {
            return Err(crate::error::AppError::Internal(format!(
                "Unsupported refresh rate: {refresh_rate}Hz"
            )));
        }

        if self.profile_is_active(profile) {
            self.display().set_refresh_rate(refresh_rate);
        } else {
            self.app_config.profile_mut(profile).screen_refresh = refresh_rate;
            self.persist_config();
        }
        Ok(())
    }

    fn set_keyboard_brightness_for_profile(
        &mut self,
        profile: PowerProfile,
        brightness: u8,
    ) -> crate::error::AppResult<()> {
        if self.profile_is_active(profile) {
            self.kb().set_keyboard_brightness(brightness);
        } else {
            self.app_config.profile_mut(profile).key_lvl = brightness;
            self.persist_config();
        }
        Ok(())
    }

    fn set_rgb_effect_for_profile(
        &mut self,
        profile: PowerProfile,
        effect: RGBEffect,
    ) -> crate::error::AppResult<()> {
        if self.profile_is_active(profile) {
            self.kb().set_rgb_effect(effect);
        } else {
            self.app_config
                .profile_mut(profile)
                .rgb_effect
                .set(&effect)?;
            self.persist_config();
        }
        Ok(())
    }

    fn set_under_glow_for_profile(
        &mut self,
        profile: PowerProfile,
        enabled: bool,
    ) -> crate::error::AppResult<()> {
        if self.profile_is_active(profile) {
            self.kb().set_under_glow_enabled(enabled);
        } else {
            self.app_config.profile_mut(profile).vc_lvl = if enabled { 255 } else { 0 };
            self.persist_config();
        }
        Ok(())
    }

    fn set_battery_limit(&mut self, limit: BatteryLimit) -> crate::error::AppResult<()> {
        self.battery().set_battery_limit(limit);
        Ok(())
    }

    fn display_layout_changed(&mut self) {
        if let Err(error) = self.display_manager.refresh_primary() {
            warn!(%error, "Display layout changed but primary display could not be refreshed");
        }

        let mut state = self.app_config.read();
        if state.rgb_effect.value() == RGBEffect::Ambient {
            AmbientEffect::start(crate::razer::device_handle::device());
        }
    }

    fn set_default_multimedia_keys(&mut self, enabled: bool) -> crate::error::AppResult<()> {
        self.kb().set_default_multimedia_keys(enabled);
        Ok(())
    }

    fn set_theme_color(&mut self, color: ThemeColor) -> crate::error::AppResult<()> {
        disable_osd! {
            self.app_config.theme_color = color;
            let rgb_effect = self.app_config.get().rgb_effect.value();
            self.kb().set_rgb_effect(rgb_effect);
            crate::ui::theme::set_runtime_theme_color(color);
            self.persist_config();
        };
        Ok(())
    }

    fn profile_is_active(&self, profile: PowerProfile) -> bool {
        AppConfig::active_profile() == profile
    }

    #[instrument(skip(self), fields(notify_startup))]
    fn initialize(&mut self, notify_startup: bool) {
        PersistBuffer::disable();
        let mut state = self.app_config.read();

        disable_osd! {
            // --- Screen Brightness ---
            self.brightness_worker
                .set_screen_brightness(state.screen_lvl);

            // --- Keyboard Configuration ---
            self.kb().keyboard_control(true);
            self.kb().enable_multimedia_keys();
            self.kb().set_rgb_effect(state.rgb_effect.value());
            self.kb().enable_under_glow(state.vc_lvl);
            self.kb().set_keyboard_brightness(state.key_lvl);

            // --- System Performance & Power ---
            let _ = self.perf().set_perf_mode(state.perf_mode.value());
            let limit = self.app_config.battery_limit.value();
            self.battery().set_battery_limit(limit);

            // --- Function / Multimedia Key Mapping ---
            if self.app_config.default_multimedia_keys {
                self.kb().enable_multimedia_keys();
            } else {
                self.kb().restore_fn_keys();
            }
        };

        if notify_startup {
            app(OsdEvent::Startup.into());
        }
        PersistBuffer::enable();

        // --- Display Refresh Rate ---
        if state.screen_refresh != 0 {
            self.display().set_refresh_rate(state.screen_refresh);
        } else {
            self.display().get_refresh_rate(true);
        }
        self.kb().init_keyboard_width();
        self.persist_config();
    }

    fn sleep(&mut self) -> bool {
        self.kb().set_keyboard_color(0, 0, 0, 0);
        self.kb().keyboard_control(false);
        let _ = command(self.device, 0x030a, &[5, 0], None); // reset keyboard effect
        // let _ = command(self.device, 0x0303, &[1, 5, 0], None); // turn off keyboard light (set_keyboard_color() with brightness 0 already does this)
        let _ = command(self.device, 0x0300, &[1, 38, 0], None); // set underglow brightness to 0
        let _ = command(self.device, 0x0303, &[1, 38, 0], None); // turn off underglow brightness
        let _ = command(self.device, 0x0d02, &[1, 0, 6, 0], None); // reset perf mode
        AmbientEffect::stop();
        true
    }

    fn shutdown(&mut self) -> bool {
        AmbientEffect::stop();
        self.kb().restore_fn_keys();
        self.kb().keyboard_control(false);
        let _ = command(self.device, 0x030a, &[RGBEffect::Cycle as u8, 0], None);
        true
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn persist_config(&mut self) {
        persist_config(self.app_config, &self.persist_buffer);
    }
}
