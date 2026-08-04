impl<'a> Executer<'a> {
    pub fn new(
        device: &'a mut Device,
        app_config: &'a mut AppConfig,
        persist_buffer: PersistBuffer,
        rx: Receiver<DeviceCmd>,
        urgent_rx: Receiver<DeviceCmd>,
    ) -> Result<Self, AppError> {
        let display_manager = DisplayManager::new().ok_or(AppError::DisplayNotFound)?;
        crate::ui::theme::set_runtime_theme_color(app_config.theme_color);
        Ok(Self {
            device,
            app_config,
            persist_buffer,
            rx,
            urgent_rx,
            brightness_worker: BrightnessWorker::new(),
            display_manager,
            refresh_cycle_timeout: Instant::now(),
            battery_cycle_timeout: Instant::now(),
            fan_speed_limits: FanSpeedLimits::default(),
        })
    }
    pub fn process_commands(&mut self) {
        loop {
            // Sleep cleanup must not wait behind regular UI, monitor, or ambient
            // commands once Windows begins its suspend notification.
            if let Ok(cmd) = self.urgent_rx.try_recv() {
                if !self.dispatch(cmd) {
                    break;
                }
                continue;
            }

            match self.rx.recv_timeout(Duration::from_millis(50)) {
                Ok(cmd) => {
                    if !self.dispatch(cmd) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        info!("All DeviceHandle senders dropped; device worker thread exiting");
    }
    fn kb(&mut self) -> KeyboardHandler<'_> {
        KeyboardHandler::new(self.device, self.app_config, &self.persist_buffer)
    }
    fn perf(&mut self) -> PerformanceHandler<'_> {
        PerformanceHandler::new(
            self.device,
            self.app_config,
            &self.persist_buffer,
            self.fan_speed_limits,
        )
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
        BatteryHandler::new(self.device, &mut self.battery_cycle_timeout)
    }
}
