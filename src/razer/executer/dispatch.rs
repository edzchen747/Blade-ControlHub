impl<'a> Executer<'a> {
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
            DeviceCmd::SetCustomModeConfig(cpu_level, gpu_level, tx) => {
                let _ = tx.send(self.set_custom_mode_config(cpu_level, gpu_level));
            }
            DeviceCmd::SetFanSpeed(profile, speed, tx) => {
                let _ = tx.send(self.set_fan_speed_for_profile(profile, speed));
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
                let battery_limit = match self.battery().battery_limit() {
                    Ok(limit) => limit,
                    Err(error) => {
                        warn!(%error, "Failed to read device battery-care setting");
                        BatteryLimit::Unknown
                    }
                };
                let state = crate::runtime::settings_state::SettingsState::from_config_with_fan_speed_limits_and_battery_limit(
                    self.app_config.clone(),
                    supported_refresh_rates,
                    self.fan_speed_limits,
                    battery_limit,
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
}
