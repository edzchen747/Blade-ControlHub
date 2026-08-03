// ── Device Commands ─────────────────────────────────────────────────────────

/// Commands that can be sent to the background device thread.
pub enum DeviceCmd {
    InitializeDevice(bool),
    SleepDevice(mpsc::Sender<bool>),
    AdjustKeyboardLight(bool),
    GetPID(mpsc::Sender<u16>),
    GetModelName(mpsc::Sender<String>),
    #[allow(dead_code)]
    GetPerfMode(mpsc::Sender<AppResult<PerfMode>>),
    GetPrimaryMultimediaKeys(mpsc::Sender<bool>),
    TogglePrimaryMultimediaKeys(mpsc::Sender<bool>),
    SetPrimaryMultimediaKeys(bool, mpsc::Sender<AppResult<()>>),
    SetMuteIndicator(AudioType, bool),
    CycleBatteryLimit,
    SetBatteryLimit(BatteryLimit, mpsc::Sender<AppResult<()>>),
    SetThemeColor(ThemeColor, mpsc::Sender<AppResult<()>>),
    CycleRGBMode,
    SetRGBMode(PowerProfile, RGBEffect, mpsc::Sender<AppResult<()>>),
    CyclePerfMode,
    SetPerfMode(PowerProfile, PerfMode, mpsc::Sender<AppResult<()>>),
    SetCustomModeConfig(u8, u8, mpsc::Sender<AppResult<()>>),
    SetFanSpeed(PowerProfile, u8, mpsc::Sender<AppResult<()>>),
    ToggleUnderGlow,
    SetUnderGlow(PowerProfile, bool, mpsc::Sender<AppResult<()>>),
    AdjustScreenBrightness(i8),
    CycleRefreshRate,
    SetRefreshRate(PowerProfile, u32, mpsc::Sender<AppResult<()>>),
    DisplayLayoutChanged,
    SetKeyboardBrightness(PowerProfile, u8, mpsc::Sender<AppResult<()>>),
    SetKeyboardColor(u8, u8, u8, u8),
    #[allow(dead_code)]
    SetLidLogo(LidLogoMode),
    PersistConfig,
    GetConfig(mpsc::Sender<AppConfig>),
    GetSettingsState(mpsc::Sender<SettingsState>),
    Shutdown(mpsc::Sender<bool>),
}

// ── DeviceHandle ────────────────────────────────────────────────────────────

/// A thread-safe, cloneable handle for sending commands to the device thread.
#[derive(Debug, Clone)]
pub struct DeviceHandle {
    sender: Sender<DeviceCmd>,
    urgent_sender: Sender<DeviceCmd>,
}

impl DeviceHandle {
    // ── Queries (blocking) ──────────────────────────────────────────

    pub fn get_pid(&self) -> AppResult<u16> {
        self.query(DeviceCmd::GetPID)
    }

    pub fn get_model_name(&self) -> AppResult<String> {
        self.query(DeviceCmd::GetModelName)
    }

    #[allow(dead_code)]
    pub fn get_perf_mode(&self) -> AppResult<PerfMode> {
        self.query_result(DeviceCmd::GetPerfMode)
    }

    #[allow(dead_code)]
    pub fn get_primary_multimedia_keys(&self) -> AppResult<bool> {
        self.query(DeviceCmd::GetPrimaryMultimediaKeys)
    }

    pub fn toggle_primary_multimedia_keys(&self) -> AppResult<bool> {
        self.query(DeviceCmd::TogglePrimaryMultimediaKeys)
    }

    pub fn set_primary_multimedia_keys(&self, enabled: bool) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetPrimaryMultimediaKeys(enabled, tx))
    }

    pub fn get_config(&self) -> AppResult<AppConfig> {
        self.query(DeviceCmd::GetConfig)
    }

    pub fn get_settings_state(&self) -> AppResult<SettingsState> {
        self.query(DeviceCmd::GetSettingsState)
    }

    pub fn shutdown(&self) -> AppResult<bool> {
        self.query(DeviceCmd::Shutdown)
    }

    pub fn sleep(&self) -> AppResult<bool> {
        self.query_urgent(DeviceCmd::SleepDevice)
    }

    // ── Fire-and-forget commands ────────────────────────────────────

    pub fn initialize(&self, notify_startup: bool) {
        self.send(DeviceCmd::InitializeDevice(notify_startup));
    }

    pub fn keyboard_light_up(&self) {
        self.send(DeviceCmd::AdjustKeyboardLight(true));
    }

    pub fn keyboard_light_down(&self) {
        self.send(DeviceCmd::AdjustKeyboardLight(false));
    }

    pub fn set_keyboard_color(&self, r: u8, g: u8, b: u8, brightness: u8) {
        self.send(DeviceCmd::SetKeyboardColor(r, g, b, brightness));
    }

    pub fn set_keyboard_brightness(&self, profile: PowerProfile, brightness: u8) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetKeyboardBrightness(profile, brightness, tx))
    }

    #[allow(dead_code)]
    pub fn set_lid_logo(&self, mode: LidLogoMode) {
        self.send(DeviceCmd::SetLidLogo(mode));
    }

    pub fn set_speakers_mute_indicator(&self, muted: bool) {
        self.send(DeviceCmd::SetMuteIndicator(AudioType::Speakers, muted));
    }

    pub fn set_mic_mute_indicator(&self, muted: bool) {
        self.send(DeviceCmd::SetMuteIndicator(AudioType::Mic, muted));
    }

    pub fn cycle_rgb_mode(&self) {
        self.send(DeviceCmd::CycleRGBMode);
    }

    pub fn set_rgb_mode(&self, profile: PowerProfile, effect: RGBEffect) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetRGBMode(profile, effect, tx))
    }

    pub fn cycle_perf_mode(&self) {
        self.send(DeviceCmd::CyclePerfMode);
    }

    pub fn set_perf_mode(&self, profile: PowerProfile, mode: PerfMode) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetPerfMode(profile, mode, tx))
    }

    pub fn set_custom_mode_config(&self, cpu_level: u8, gpu_level: u8) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetCustomModeConfig(cpu_level, gpu_level, tx))
    }

    pub fn set_fan_speed(&self, profile: PowerProfile, speed: u8) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetFanSpeed(profile, speed, tx))
    }

    pub fn toggle_vc(&self) {
        self.send(DeviceCmd::ToggleUnderGlow);
    }

    pub fn set_under_glow(&self, profile: PowerProfile, enabled: bool) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetUnderGlow(profile, enabled, tx))
    }

    pub fn adjust_screen_brightness(&self, change: i8) {
        self.send(DeviceCmd::AdjustScreenBrightness(change));
    }

    pub fn cycle_refresh_rate(&self) {
        self.send(DeviceCmd::CycleRefreshRate);
    }

    pub fn set_refresh_rate(&self, profile: PowerProfile, refresh_rate: u32) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetRefreshRate(profile, refresh_rate, tx))
    }

    pub fn display_layout_changed(&self) {
        self.send(DeviceCmd::DisplayLayoutChanged);
    }

    pub fn cycle_battery_limit(&self) {
        self.send(DeviceCmd::CycleBatteryLimit);
    }

    pub fn set_battery_limit(&self, limit: BatteryLimit) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetBatteryLimit(limit, tx))
    }

    pub fn set_theme_color(&self, color: ThemeColor) -> AppResult<()> {
        self.query_result(|tx| DeviceCmd::SetThemeColor(color, tx))
    }

    pub fn persist_config(&self) {
        self.send(DeviceCmd::PersistConfig);
    }

    // ── Internal helpers ────────────────────────────────────────────

    /// Sends a command and logs an error if the device thread has exited.
    fn send(&self, cmd: DeviceCmd) {
        if let Err(error) = self.sender.send(cmd) {
            warn!(?error, "Device worker is unavailable; dropping command");
        }
    }

    /// Sends a query command and blocks until the response arrives (5s timeout).
    fn query<T, F>(&self, make_query: F) -> AppResult<T>
    where
        F: FnOnce(mpsc::Sender<T>) -> DeviceCmd,
    {
        let (resp_tx, resp_rx) = mpsc::channel::<T>();
        let cmd = make_query(resp_tx);

        if let Err(error) = self.sender.send(cmd) {
            warn!(
                ?error,
                "Device worker is unavailable; query cannot be delivered"
            );
            return Err(device_worker_unavailable());
        }

        match resp_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(value) => Ok(value),
            Err(RecvTimeoutError::Timeout) => Err(AppError::HardwareTimeout),
            Err(RecvTimeoutError::Disconnected) => Err(device_worker_unavailable()),
        }
    }

    fn query_urgent<T, F>(&self, make_query: F) -> AppResult<T>
    where
        F: FnOnce(mpsc::Sender<T>) -> DeviceCmd,
    {
        let (resp_tx, resp_rx) = mpsc::channel::<T>();
        let cmd = make_query(resp_tx);

        if let Err(error) = self.urgent_sender.send(cmd) {
            warn!(
                ?error,
                "Device worker is unavailable; urgent query cannot be delivered"
            );
            return Err(device_worker_unavailable());
        }

        match resp_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(value) => Ok(value),
            Err(RecvTimeoutError::Timeout) => Err(AppError::HardwareTimeout),
            Err(RecvTimeoutError::Disconnected) => Err(device_worker_unavailable()),
        }
    }

    fn query_result<T, F>(&self, make_query: F) -> AppResult<T>
    where
        F: FnOnce(mpsc::Sender<AppResult<T>>) -> DeviceCmd,
    {
        let (resp_tx, resp_rx) = mpsc::channel::<AppResult<T>>();
        let cmd = make_query(resp_tx);

        if let Err(error) = self.sender.send(cmd) {
            warn!(
                ?error,
                "Device worker is unavailable; query cannot be delivered"
            );
            return Err(device_worker_unavailable());
        }

        match resp_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(AppError::HardwareTimeout),
            Err(RecvTimeoutError::Disconnected) => Err(device_worker_unavailable()),
        }
    }
}

