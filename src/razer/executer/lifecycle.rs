impl<'a> Executer<'a> {
    fn reinitialize(&mut self) -> Result<u16, AppError> {
        let device = crate::razer::device_handle::get_razer_device()?;
        let pid = device.info.pid;
        let model_name = device.info.name.to_string();

        *self.device = device;
        self.app_config
            .set_device_model(format!("0x{pid:04x}"), model_name);
        self.initialize(false);
        info!(pid = format_args!("0x{pid:04x}"), "Reopened Razer HID device after system wake");
        Ok(pid)
    }

    #[instrument(skip(self), fields(notify_startup))]
    fn initialize(&mut self, notify_startup: bool) {
        PersistBuffer::disable();
        self.refresh_fan_speed_limits();
        let mut state = self.app_config.read();

        disable_osd! {
            self.brightness_worker
                .set_screen_brightness(state.screen_lvl);

            self.kb().keyboard_control(true);
            self.kb().enable_multimedia_keys();
            self.kb().set_rgb_effect(state.rgb_effect.value());
            self.kb().enable_under_glow(state.vc_lvl);
            self.kb().set_keyboard_brightness(state.key_lvl);

            let _ = self.perf().set_perf_mode(state.perf_mode.value());
            if self.app_config.primary_multimedia_keys {
                self.kb().enable_multimedia_keys();
            } else {
                self.kb().restore_fn_keys();
            }
        };

        if notify_startup {
            app(OsdEvent::Startup.into());
        }
        PersistBuffer::enable();

        if state.screen_refresh != 0 {
            self.display().set_refresh_rate(state.screen_refresh);
        } else {
            self.display().get_refresh_rate(true);
        }
        self.kb().init_keyboard_width();
        self.persist_config();
    }

    fn refresh_fan_speed_limits(&mut self) {
        match self.perf().get_fan_speed_limits() {
            Ok(limits) => {
                self.fan_speed_limits = limits;
                for profile in [PowerProfile::Ac, PowerProfile::Battery] {
                    self.app_config
                        .profile_mut(profile)
                        .fan_speeds
                        .clamp_to_limits(limits);
                }
                info!(
                    min = limits.min,
                    max = limits.max,
                    "Loaded firmware fan-speed limits"
                );
            }
            Err(error) => {
                warn!(
                    %error,
                    min = self.fan_speed_limits.min,
                    max = self.fan_speed_limits.max,
                    "Failed to read firmware fan-speed limits; using session defaults"
                );
            }
        }
    }

    fn sleep(&mut self) -> bool {
        AmbientEffect::stop();
        self.kb().set_keyboard_color(0, 0, 0, 0);
        self.kb().keyboard_control(false);
        let _ = command(self.device, 0x030a, &[5, 0], None); // reset keyboard effect
        let _ = command(self.device, 0x0300, &[1, 38, 0], None); // set underglow brightness to 0
        let _ = command(self.device, 0x0303, &[1, 38, 0], None); // turn off underglow brightness
        reset_perf_mode_for_sleep(self.device);
        true
    }

    fn shutdown(&mut self) -> bool {
        AmbientEffect::stop();
        self.kb().restore_fn_keys();
        self.kb().keyboard_control(false);
        let _ = command(self.device, 0x030a, &[RGBEffect::Cycle as u8, 0], None);
        true
    }


    fn persist_config(&mut self) {
        persist_config(self.app_config, &self.persist_buffer);
    }
}
