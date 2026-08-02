impl SettingsApp {
    fn new(state: Option<SettingsState>) -> Self {
        let settings = SettingsStore::new();
        let (razer_key_capture_tx, razer_key_capture_rx) = mpsc::channel();
        let (settings_update_tx, settings_update_rx) = mpsc::channel();
        let settings_update_shutdown = Arc::new(AtomicBool::new(false));
        if let Some(state) = state.clone() {
            settings.show(state);
        } else {
            settings.with_settings(|settings| {
                settings.show = true;
                settings.update = true;
                settings.state = None;
            });
        }
        Self {
            settings,
            state_loaded: state.is_some(),
            last_frame_at: None,
            settings_update_tx,
            settings_update_rx,
            settings_update_shutdown,
            settings_update_thread: None,
            razer_key_capture_tx,
            razer_key_capture_rx,
            razer_key_capture_cancel: None,
            razer_key_capture_id: 0,
            active_razer_key_capture_id: None,
            applied_window_icon_color: None,
            native_window_icons: None,
            reported_window_focus: None,
        }
    }

    fn start_settings_update_worker(&mut self, ctx: egui::Context) {
        if self.settings_update_thread.is_some() {
            return;
        }

        self.settings_update_thread = spawn_settings_update_worker(
            self.state_loaded,
            self.settings_update_shutdown.clone(),
            self.settings_update_tx.clone(),
            ctx,
        );
    }

    fn sync_osd_suppression(&mut self, focused: bool) {
        if self.reported_window_focus == Some(focused) {
            return;
        }

        match client::set_settings_window_state(true, focused) {
            Ok(()) => self.reported_window_focus = Some(focused),
            Err(error) => {
                warn!(%error, focused, "Failed to update settings window OSD suppression")
            }
        }
    }

    fn process_backend(&mut self, ctx: &egui::Context) {
        self.drain_settings_update_messages(ctx);
        self.drain_razer_key_capture_messages(ctx);

        let commands = self
            .settings
            .with_settings(|settings| settings.drain_commands());
        let mut command_sent = false;

        for command in commands {
            match command {
                SettingsCommand::SetDefaultMultimediaKeys(enabled) => {
                    if let Err(error) = client::set_default_multimedia_keys(enabled) {
                        warn!(%error, "Failed to update default multimedia key setting");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetPerfMode(profile, mode) => {
                    if let Err(error) = client::set_perf_mode(profile, mode) {
                        warn!(%error, "Failed to update performance mode");
                        self.handle_failed_perf_mode_update(mode, ctx);
                        command_sent = true;
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetCustomModeConfig {
                    cpu_level,
                    gpu_level,
                } => {
                    if let Err(error) = client::set_custom_mode_config(cpu_level, gpu_level) {
                        warn!(%error, "Failed to update custom performance mode configuration");
                        command_sent = true;
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetFanSpeed(profile, speed) => {
                    if let Err(error) = client::set_fan_speed(profile, speed) {
                        warn!(%error, "Failed to update fan speed");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetRefreshRate(profile, hz) => {
                    if let Err(error) = client::set_refresh_rate(profile, hz) {
                        warn!(%error, "Failed to update refresh rate");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetKeyboardBrightness(profile, level) => {
                    if let Err(error) = client::set_keyboard_brightness(profile, level) {
                        warn!(%error, "Failed to update keyboard brightness");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetRgbEffect(profile, effect) => {
                    if let Err(error) = client::set_rgb_effect(profile, effect) {
                        warn!(%error, "Failed to update RGB effect");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetUnderGlow(profile, enabled) => {
                    if let Err(error) = client::set_under_glow(profile, enabled) {
                        warn!(%error, "Failed to update vapour chamber light");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetBatteryLimit(limit) => {
                    if let Err(error) = client::set_battery_limit(limit) {
                        warn!(%error, "Failed to update battery charge limit");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetThemeColor(color) => {
                    if let Err(error) = client::set_theme_color(color) {
                        warn!(%error, "Failed to update theme color");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::BeginRazerKeyCapture { row_idx } => {
                    self.start_razer_key_capture(row_idx, ctx);
                }
                SettingsCommand::CancelRazerKeyCapture => {
                    self.cancel_razer_key_capture(ctx);
                }
            }
        }

        if command_sent {
            settings_updates::notify_settings_updated();
        }
    }

    fn start_razer_key_capture(&mut self, row_idx: usize, ctx: &egui::Context) {
        if let Some(cancel) = self.razer_key_capture_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }

        self.razer_key_capture_id = self.razer_key_capture_id.saturating_add(1);
        let capture_id = self.razer_key_capture_id;
        self.active_razer_key_capture_id = Some(capture_id);
        let after_unix_ms = current_unix_ms();
        self.settings.with_settings(|settings| {
            settings.custom_key_map.special_key = None;
            settings.custom_key_map.set_listening_idx(Some(row_idx));
        });
        ctx.request_repaint();

        let cancel = Arc::new(AtomicBool::new(false));
        self.razer_key_capture_cancel = Some(cancel.clone());
        let tx = self.razer_key_capture_tx.clone();
        let worker_ctx = ctx.clone();

        if let Err(error) = thread::Builder::new()
            .name("blade-settings-razer-key-capture".to_string())
            .spawn(move || {
                run_razer_key_capture_worker(capture_id, after_unix_ms, cancel, tx, worker_ctx)
            })
        {
            warn!(%error, "Failed to spawn Razer key capture worker");
            self.razer_key_capture_cancel = None;
            self.active_razer_key_capture_id = None;
            self.settings
                .with_settings(|settings| settings.custom_key_map.set_listening_idx(None));
            ctx.request_repaint();
        }
    }

    fn cancel_razer_key_capture(&mut self, ctx: &egui::Context) {
        if let Some(cancel) = self.razer_key_capture_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.active_razer_key_capture_id = None;
        self.settings
            .with_settings(|settings| settings.custom_key_map.set_listening_idx(None));
        ctx.request_repaint();

        let ctx = ctx.clone();
        if let Err(error) = thread::Builder::new()
            .name("blade-settings-razer-key-cancel".to_string())
            .spawn(move || {
                if let Err(error) = client::cancel_razer_key_capture() {
                    warn!(%error, "Failed to cancel Razer key capture");
                }
                ctx.request_repaint();
            })
        {
            warn!(%error, "Failed to spawn Razer key capture cancel worker");
        }
    }

    fn drain_razer_key_capture_messages(&mut self, ctx: &egui::Context) {
        while let Ok(message) = self.razer_key_capture_rx.try_recv() {
            if Some(message.capture_id()) != self.active_razer_key_capture_id {
                continue;
            }

            match message {
                RazerKeyCaptureMessage::Captured {
                    capture_id: _,
                    key_code,
                } => {
                    self.razer_key_capture_cancel = None;
                    self.active_razer_key_capture_id = None;
                    self.settings
                        .with_settings(|settings| settings.apply_captured_razer_key(key_code));
                    ctx.request_repaint();
                }
            }
        }
    }

    fn drain_settings_update_messages(&mut self, ctx: &egui::Context) {
        while let Ok(message) = self.settings_update_rx.try_recv() {
            match message {
                SettingsUpdateMessage::State(state) => {
                    self.settings.update_state(state);
                    self.state_loaded = true;
                    ctx.request_repaint();
                }
            }
        }
    }

    fn handle_failed_perf_mode_update(&mut self, mode: PerfMode, ctx: &egui::Context) {
        self.settings.with_settings(|settings| {
            settings.flash_unsupported_perf_mode(mode);
        });
        ctx.request_repaint();
        ctx.request_repaint_after(Settings::unsupported_perf_mode_notice_duration());
    }

    fn pace_frame(&mut self) {
        let now = Instant::now();
        if let Some(last_frame_at) = self.last_frame_at {
            let elapsed = now.saturating_duration_since(last_frame_at);
            if elapsed < SETTINGS_FRAME_INTERVAL {
                std::thread::sleep(SETTINGS_FRAME_INTERVAL - elapsed);
            }
        }
        self.last_frame_at = Some(Instant::now());
    }

    fn update_window_icon(&mut self, frame: &eframe::Frame) {
        let color = self.settings.with_settings(|settings| {
            settings
                .state
                .as_ref()
                .map(|state| state.theme_color)
                .unwrap_or(SETTINGS_LOADING_ICON_COLOR)
        });

        if self.applied_window_icon_color == Some(color) {
            return;
        }

        if let Some(icons) = NativeWindowIcons::apply(frame, color) {
            self.native_window_icons = Some(icons);
            self.applied_window_icon_color = Some(color);
        }
    }
}

