impl<'a> Executer<'a> {
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
                if mode != PerfMode::Custom {
                    self.perf().remove_perf_mode(mode);
                }
                return Err(error);
            }
        } else {
            self.app_config.profile_mut(profile).perf_mode.set(&mode)?;
            self.persist_config();
        }
        Ok(())
    }

    fn set_custom_mode_config(
        &mut self,
        cpu_level: u8,
        gpu_level: u8,
    ) -> crate::error::AppResult<()> {
        validate_custom_mode_level(cpu_level)?;
        validate_custom_mode_level(gpu_level)?;

        let mut ac_state = self.app_config.profile(PowerProfile::Ac);
        let custom_mode_is_active = self.profile_is_active(PowerProfile::Ac)
            && ac_state.perf_mode.value() == PerfMode::Custom;
        if custom_mode_is_active {
            self.perf().set_custom_mode_config(cpu_level, gpu_level)
        } else {
            self.app_config.custom_mode_config.cpu_level = cpu_level;
            self.app_config.custom_mode_config.gpu_level = gpu_level;
            self.persist_config();
            Ok(())
        }
    }

    fn set_fan_speed_for_profile(
        &mut self,
        profile: PowerProfile,
        speed: u8,
    ) -> crate::error::AppResult<()> {
        validate_fan_speed(speed, self.fan_speed_limits)?;

        if self.profile_is_active(profile) {
            self.perf().set_fan_speed(speed)
        } else {
            let state = self.app_config.profile_mut(profile);
            let perf_mode = state.perf_mode.value();
            state.fan_speeds.set(perf_mode, speed);
            self.persist_config();
            Ok(())
        }
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

    fn set_primary_multimedia_keys(&mut self, enabled: bool) -> crate::error::AppResult<()> {
        self.kb().set_primary_multimedia_keys(enabled);
        Ok(())
    }

    fn set_advanced_experimental_features(&mut self, enabled: bool) -> crate::error::AppResult<()> {
        self.app_config.advanced_experimental_features = enabled;
        self.persist_config();
        Ok(())
    }

    fn set_start_with_admin(&mut self, enabled: bool) -> crate::error::AppResult<()> {
        self.app_config.start_with_admin = enabled;

        crate::config::persist_config_now(self.app_config);
        self.persist_config();

        // Only DISABLING refreshes the task here, otherwise UAC prompts twice
        // (once for the task, once for the app); the relaunched process
        // refreshes the task on launch instead.
        if !enabled {
            let start_with_windows = self.app_config.start_with_windows;
            if let Err(error) = std::thread::Builder::new()
                .name("blade-admin-task-refresh".to_string())
                .spawn(move || {
                    crate::win::system::startup::Startup::refresh_now(start_with_windows, false);
                })
            {
                warn!(%error, "Failed to spawn startup task refresh thread");
            }
        }
        Ok(())
    }

    fn set_start_with_windows(&mut self, enabled: bool) {
        self.app_config.start_with_windows = enabled;
        self.persist_config();

        if enabled {
            crate::win::system::startup::Startup::register(self.app_config.start_with_admin);
        } else {
            crate::win::system::startup::Startup::unregister();
        }
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
}
