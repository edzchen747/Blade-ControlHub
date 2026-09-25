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

/// Whether a device setting was written by the settings window rather than by a
/// key, a monitor or a lighting effect.
///
/// The window is the only caller that names a value outright: a key cycles,
/// toggles or adjusts, and the monitors re-apply a whole profile. The shape of
/// the command is therefore a reliable statement of where it came from, and the
/// queue is the last place where that is still known.
///
/// Only the settings are listed, plus a custom control the window put on a
/// named side: its switch already shows the new state, while the same control
/// flipped by a key has nothing else to show for itself. The window's other
/// commands — key bindings, the startup toggles, Command Lab, a state
/// snapshot — raise no overlay for there to be a question about.
fn window_originated(cmd: &DeviceCmd) -> bool {
    matches!(
        cmd,
        DeviceCmd::SetPerfMode(..)
            | DeviceCmd::SetCustomModeConfig(..)
            | DeviceCmd::SetFanSpeed(..)
            | DeviceCmd::SetRefreshRate(..)
            | DeviceCmd::SetKeyboardBrightness(..)
            | DeviceCmd::SetRGBMode(..)
            | DeviceCmd::SetUnderGlow(..)
            | DeviceCmd::SetBatteryLimit(..)
            | DeviceCmd::SetThemeColor(..)
            | DeviceCmd::SetCustomToggle(..)
    )
}

impl<'a> Executer<'a> {
    /// Runs one queued command, suppressing the OSD for the ones the settings
    /// window sent.
    ///
    /// The overlay exists for feedback the user has no other way to see. A
    /// control in the window already shows its own new value, so an overlay on
    /// top of it is noise — but a Razer special key or an Fn combination has
    /// nothing else to show for itself, and has to keep its overlay even while
    /// the window is open and focused. Which surface asked is therefore the
    /// honest test, not which surface has focus.
    fn dispatch(&mut self, cmd: DeviceCmd) -> bool {
        if window_originated(&cmd) {
            disable_osd! { self.dispatch_command(cmd) }
        } else {
            self.dispatch_command(cmd)
        }
    }

    fn dispatch_command(&mut self, cmd: DeviceCmd) -> bool {
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
            DeviceCmd::ToggleUnderGlow => {
                if self.has_vapour_chamber() {
                    self.kb().toggle_under_glow();
                }
            }
            DeviceCmd::SetUnderGlow(profile, enabled, tx) => {
                let _ = tx.send(self.set_under_glow_for_profile(profile, enabled));
            }
            DeviceCmd::SetKeyboardColor(r, g, b, brightness) => {
                self.kb().set_keyboard_color(r, g, b, brightness)
            }
            DeviceCmd::SetKeyRows(rows, reassert_brightness) => {
                // Dropped once the effect has been stopped, so a frame queued
                // before a sleep cannot repaint the keyboard after the blackout.
                if AudioBloomEffect::is_active() {
                    self.kb().set_key_rows(&rows, reassert_brightness);
                }
                AudioBloomEffect::frame_written();
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
            DeviceCmd::ReplaySavedCapture(name) => {
                // The device thread owns the config, so a binding names the
                // capture and the lookup happens here rather than in the
                // caller, which would need its own copy.
                if !self.replay_saved_capture(&name) {
                    warn!(name, "A key binding names a capture that no longer exists");
                }
            }
            DeviceCmd::SetCustomToggles(toggles) => {
                self.app_config.custom_toggles = toggles;
                self.persist_config();
            }
            DeviceCmd::SetCustomToggle(name, enabled) => {
                self.apply_custom_toggle(&name, Some(enabled));
            }
            DeviceCmd::ToggleCustomControl(name) => self.apply_custom_toggle(&name, None),
            DeviceCmd::SetHiddenDashboardControls(hidden) => {
                self.app_config.hidden_dashboard_controls = hidden;
                self.persist_config();
            }
            DeviceCmd::SetKeyBindings(bindings) => {
                crate::win::input::custom_bindings::replace(&bindings);
                self.app_config.key_bindings = bindings;
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
            DeviceCmd::CycleBatteryLimit => {
                let limit = self.battery().cycle_battery_limit();
                self.record_battery_limit(limit);
            }
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

    /// Puts one custom control on a side and replays the capture for it.
    ///
    /// `enabled` is the side to land on, or `None` to flip whichever side it is
    /// on now — all a key binding knows to ask for, since the remembered state
    /// lives here with the config.
    fn apply_custom_toggle(&mut self, name: &str, enabled: Option<bool>) {
        let Some(toggle) = self
            .app_config
            .custom_toggles
            .iter_mut()
            .find(|toggle| toggle.name == name)
        else {
            warn!(name, "No custom control by that name");
            return;
        };

        // The remembered side is written even when the capture has gone
        // missing, so the switch the user just flipped stays flipped and the
        // page can say what is wrong with it.
        let enabled = enabled.unwrap_or(!toggle.enabled);
        toggle.enabled = enabled;
        let capture = toggle.capture_for(enabled).to_string();
        self.persist_config();

        if !capture.is_empty() && self.replay_saved_capture(&capture) {
            app(OsdEvent::CustomControl(name.to_owned(), enabled).into());
        } else {
            warn!(
                control = name,
                capture, "A custom control names a capture that no longer exists"
            );
            app(OsdEvent::CustomAction(format!("{name} failed")).into());
        }
    }

    /// Replays a capture the user saved on the Command Lab page, reporting
    /// whether it still exists. A command that fails is logged and the rest of
    /// the capture still runs, so one rejected report does not strand the device
    /// half-way through a toggle.
    fn replay_saved_capture(&mut self, name: &str) -> bool {
        let Some(commands) = self.app_config.command_lab_commands.get(name).cloned() else {
            return false;
        };
        for captured in commands {
            if let Err(error) = command(self.device, captured.command, &captured.args, None) {
                warn!(
                    %error,
                    command = captured.command,
                    capture = name,
                    "Saved capture replay command failed"
                );
            }
        }
        true
    }
}
