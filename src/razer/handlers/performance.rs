use librazer::device::Device;
use tracing::info;

use crate::{
    config::persist_config,
    razer::{config::AppConfig, enums::PerfMode, protocol::command},
    ui::{app::app, app_events::OsdEvent},
    utils::persist::PersistBuffer,
};

/// Performance mode handler.
///
/// Accepts references to the device, app config, and persist buffer.
/// All logic is copied exactly from Executer — zero-cost abstraction.
pub struct PerformanceHandler<'a> {
    device: &'a Device,
    app_config: &'a mut AppConfig,
    persist_buffer: &'a PersistBuffer,
}

impl<'a> PerformanceHandler<'a> {
    pub fn new(
        device: &'a Device,
        app_config: &'a mut AppConfig,
        persist_buffer: &'a PersistBuffer,
    ) -> Self {
        Self {
            device,
            app_config,
            persist_buffer,
        }
    }

    // ── Performance ─────────────────────────────────────────────────────

    pub fn cycle_perf_mode(&mut self) {
        let new_perf_mode = self.app_config.get().perf_mode.next();
        self.set_perf_mode(new_perf_mode);
    }

    pub fn set_perf_mode(&mut self, perf_mode: PerfMode) {
        info!(mode = %perf_mode, "Setting performance mode");
        let _ = command(self.device, 0x0d02, &[1, 0, perf_mode as u8, 0], None);
        let perf_mode = self.get_perf_mode();
        // value comes from hardware readback; mismatch is non-fatal
        let _ = self.app_config.get().perf_mode.set(&perf_mode);
        self.persist_config();
    }

    pub fn get_perf_mode(&self) -> PerfMode {
        // unwrap_or(0): returns PerfMode 0 on hardware/protocol failure
        let perf_mode: PerfMode = command(self.device, 0x0d82, &[0, 0, 0, 0], Some(2))
            .unwrap_or(0)
            .into();
        app(OsdEvent::PerfMode(perf_mode).into());
        perf_mode
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn persist_config(&mut self) {
        persist_config(self.app_config, self.persist_buffer);
    }
}
