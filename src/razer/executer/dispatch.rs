/// The battery-care query is device-backed; settings snapshots run it only
/// once every this many `GetSettingsState` calls and reuse the cached result
/// in between.
const SETTINGS_STATE_BATTERY_QUERY_PERIOD: u32 = 20;

/// Whether the device-backed battery-care query should run for the
/// `queries`-th settings snapshot (counting from 1). The very first snapshot
/// always queries so the settings UI never starts with Unknown; afterwards it
/// runs once every period.
fn should_query_battery_limit(queries: u32) -> bool {
    queries > 0 && (queries == 1 || queries.is_multiple_of(SETTINGS_STATE_BATTERY_QUERY_PERIOD))
}

impl<'a> Executer<'a> {
    fn dispatch(&mut self, cmd: DeviceCmd) -> bool {
        match cmd {
            DeviceCmd::InitializeDevice(notif) => self.initialize(notif),
            DeviceCmd::SleepDevice(tx) => {
                let _ = tx.send(self.sleep());
            }
            DeviceCmd::ReinitializeDevice(tx) => {
                let _ = tx.send(self.reinitialize());
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
            DeviceCmd::PlayCommandLabCommands(commands) => {
                for captured in commands {
                    if let Err(error) =
                        command(self.device, captured.command, &captured.args, None)
                    {
                        warn!(
                            %error,
                            command = captured.command,
                            "Command Lab replay command failed"
                        );
                    }
                }
            }
            DeviceCmd::SaveCommandLabCommands(name, commands) => {
                self.app_config.command_lab_commands.insert(name, commands);
                self.persist_config();
            }
            DeviceCmd::RemoveCommandLabCommand(name) => {
                self.app_config.command_lab_commands.remove(&name);
                self.persist_config();
            }
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
            DeviceCmd::GetPrimaryMultimediaKeys(tx) => {
                let _ = tx.send(self.kb().get_primary_multimedia_keys());
            }
            DeviceCmd::TogglePrimaryMultimediaKeys(tx) => {
                let _ = tx.send(self.kb().toggle_primary_multimedia_keys());
            }
            DeviceCmd::SetPrimaryMultimediaKeys(enabled, tx) => {
                let _ = tx.send(self.set_primary_multimedia_keys(enabled));
            }
            DeviceCmd::SetAdvancedExperimentalFeatures(enabled, tx) => {
                let _ = tx.send(self.set_advanced_experimental_features(enabled));
            }
            DeviceCmd::SetStartWithAdmin(enabled, tx) => {
                let _ = tx.send(self.set_start_with_admin(enabled));
            }
            DeviceCmd::SetStartWithWindows(enabled) => {
                self.set_start_with_windows(enabled);
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
                self.settings_snapshot_queries = self.settings_snapshot_queries.wrapping_add(1);
                let battery_limit = if should_query_battery_limit(self.settings_snapshot_queries) {
                    match self.battery().battery_limit() {
                        Ok(limit) => {
                            self.cached_battery_limit = limit;
                            limit
                        }
                        Err(error) => {
                            warn!(%error, "Failed to read device battery-care setting");
                            self.cached_battery_limit
                        }
                    }
                } else {
                    self.cached_battery_limit
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
