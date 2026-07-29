use librazer::device::Device;
use tracing::{info, warn};

use crate::{
    config::persist_config,
    error::{AppError, AppResult},
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
        let attempts = self.app_config.get().perf_mode.items.len();
        for _ in 0..attempts {
            let new_perf_mode = self.app_config.get().perf_mode.next();
            match self.set_perf_mode(new_perf_mode) {
                Ok(()) => return,
                Err(error) => {
                    warn!(
                        mode = %new_perf_mode,
                        %error,
                        "Performance mode failed; removing it from this launch's cycle list"
                    );
                    self.remove_perf_mode_for_cycle_retry(new_perf_mode);
                }
            }
        }
    }

    pub fn set_perf_mode(&mut self, perf_mode: PerfMode) -> AppResult<()> {
        info!(mode = %perf_mode, "Setting performance mode");
        command(self.device, 0x0d02, &[1, 0, perf_mode as u8, 0], None)?;
        let requested_perf_mode = perf_mode;
        let actual_perf_mode = self.get_perf_mode();
        if actual_perf_mode != requested_perf_mode {
            return Err(AppError::Internal(format!(
                "Performance mode {requested_perf_mode} not supported; hardware stayed on {actual_perf_mode}"
            )));
        }

        let _ = self.app_config.get().perf_mode.set(&actual_perf_mode);
        self.persist_config();
        Ok(())
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

    pub fn remove_perf_mode(&mut self, perf_mode: PerfMode) {
        if self.app_config.get().perf_mode.remove(&perf_mode) {
            self.persist_config();
        }
    }

    fn remove_perf_mode_for_cycle_retry(&mut self, perf_mode: PerfMode) {
        if self
            .app_config
            .get()
            .perf_mode
            .remove_for_cycle_retry(&perf_mode)
        {
            self.persist_config();
        }
    }

    fn persist_config(&mut self) {
        persist_config(self.app_config, self.persist_buffer);
    }
}
