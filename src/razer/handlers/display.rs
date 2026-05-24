use librazer::device::Device;

use crate::{
    config::persist_config,
    razer::config::AppConfig,
    ui::{app::app, app_events::OsdEvent, theme::TOTAL_ANIM_TIME_MS},
    utils::persist::PersistBuffer,
    win::display::refresh_rate::DisplayManager,
};
use std::time::{Duration, Instant};

/// Display handler.
///
/// Accepts references to the device, app config, persist buffer, display manager,
/// and refresh cycle timeout. All logic is copied exactly from Executer.
pub struct DisplayHandler<'a> {
    _device: &'a Device,
    app_config: &'a mut AppConfig,
    persist_buffer: &'a PersistBuffer,
    display_manager: &'a mut DisplayManager,
    refresh_cycle_timeout: &'a mut Instant,
}

impl<'a> DisplayHandler<'a> {
    pub fn new(
        device: &'a Device,
        app_config: &'a mut AppConfig,
        persist_buffer: &'a PersistBuffer,
        display_manager: &'a mut DisplayManager,
        refresh_cycle_timeout: &'a mut Instant,
    ) -> Self {
        Self {
            _device: device,
            app_config,
            persist_buffer,
            display_manager,
            refresh_cycle_timeout,
        }
    }

    // ── Display ─────────────────────────────────────────────────────────

    pub fn get_refresh_rate(&mut self) -> u32 {
        let current = self.display_manager.get_current_rate();
        let supported = self.display_manager.get_supported_rates();
        let level = 1 + supported
            .iter()
            .position(|&rate| rate == current)
            .unwrap_or(0);
        app().send(OsdEvent::RefreshRate(current, level as u8, supported.len() as u8).into());
        self.app_config.get().screen_refresh = current;
        self.persist_config();
        *self.refresh_cycle_timeout =
            Instant::now() + Duration::from_millis(TOTAL_ANIM_TIME_MS as u64);
        current
    }

    pub fn cycle_refresh_rate(&mut self) {
        if Instant::now() < *self.refresh_cycle_timeout {
            let _ = self.display_manager.cycle_refresh_rate();
        }
        self.get_refresh_rate();
    }

    pub fn set_refresh_rate(&mut self, refresh_rate: u32) {
        let supported = self.display_manager.get_supported_rates();
        if supported.contains(&refresh_rate)
            && self.display_manager.get_current_rate() != refresh_rate
        {
            let _ = self.display_manager.set_refresh_rate(refresh_rate);
            self.get_refresh_rate();
        }
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn persist_config(&mut self) {
        persist_config(self.app_config, self.persist_buffer);
    }
}
