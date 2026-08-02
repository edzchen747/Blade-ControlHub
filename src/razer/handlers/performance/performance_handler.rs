use librazer::device::Device;
use tracing::{info, warn};

use crate::{
    config::persist_config,
    error::{AppError, AppResult},
    razer::{
        config::{AppConfig, FanSpeedLimits},
        enums::PerfMode,
        protocol::command,
    },
    ui::{app::app, app_events::OsdEvent},
    utils::persist::PersistBuffer,
};

const AUTO_FAN_SPEED_MODE: u8 = 0;
const FIXED_FAN_SPEED_MODE: u8 = 1;
const CUSTOM_MODE_CONFIG_COMMAND: u16 = 0x0d07;
const PERFORMANCE_MODE_RESULT_INDICES: [usize; 2] = [2, 3];

pub(crate) fn validate_custom_mode_level(level: u8) -> AppResult<()> {
    if level <= 3 {
        Ok(())
    } else {
        Err(AppError::Internal(format!(
            "Custom performance level must be between 0 and 3; got {level}"
        )))
    }
}

pub(crate) fn validate_fan_speed(speed: u8, limits: FanSpeedLimits) -> AppResult<()> {
    if speed == 0 || limits.contains(speed) {
        Ok(())
    } else {
        Err(AppError::Internal(format!(
            "Fan speed must be 0 (automatic) or between {} and {}; got {speed}",
            limits.min, limits.max
        )))
    }
}

/// Restores the firmware's sleep performance state without changing saved
/// configuration.
pub(crate) fn reset_perf_mode_for_sleep(device: &Device) {
    let _ = command(
        device,
        0x0d02,
        &performance_mode_command_args(PerfMode::Quiet, 0),
        None,
    );
}

/// Performance mode handler.
///
/// Accepts references to the device, app config, and persist buffer.
/// All logic is copied exactly from Executer — zero-cost abstraction.
pub struct PerformanceHandler<'a> {
    device: &'a Device,
    app_config: &'a mut AppConfig,
    persist_buffer: &'a PersistBuffer,
    fan_speed_limits: FanSpeedLimits,
}

impl<'a> PerformanceHandler<'a> {
    pub fn new(
        device: &'a Device,
        app_config: &'a mut AppConfig,
        persist_buffer: &'a PersistBuffer,
        fan_speed_limits: FanSpeedLimits,
    ) -> Self {
        Self {
            device,
            app_config,
            persist_buffer,
            fan_speed_limits,
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
        let fan_speed = self.app_config.get().fan_speeds.get(perf_mode);
        command(
            self.device,
            0x0d02,
            &performance_mode_command_args(perf_mode, fan_speed),
            None,
        )?;
        let requested_perf_mode = perf_mode;
        let actual_perf_mode = self.get_perf_mode()?;
        if actual_perf_mode != requested_perf_mode {
            return Err(AppError::Internal(format!(
                "Performance mode {requested_perf_mode} not supported; hardware stayed on {actual_perf_mode}"
            )));
        }

        let _ = self.app_config.get().perf_mode.set(&actual_perf_mode);
        if actual_perf_mode == PerfMode::Custom {
            let custom = self.app_config.custom_mode_config.clone();
            self.set_custom_mode_config(custom.cpu_level, custom.gpu_level)?;
        }
        let fan_speed = self.get_fan_speed();
        self.apply_fan_speed(fan_speed)?;
        self.persist_config();
        Ok(())
    }

    pub fn get_perf_mode(&self) -> AppResult<PerfMode> {
        let response = command(
            self.device,
            0x0d82,
            &performance_mode_query_args(),
            // Byte 3 is the current fan-mode result, not an echoed request
            // byte. It can be nonzero even though the query sends zero.
            Some(&PERFORMANCE_MODE_RESULT_INDICES),
        )?;
        let perf_mode = response
            .first()
            .copied()
            .ok_or_else(|| AppError::Internal("Performance-mode response was empty".to_string()))?
            .into();
        app(OsdEvent::PerfMode(perf_mode).into());
        Ok(perf_mode)
    }

    /// Applies the CPU and GPU limits for Custom mode without re-selecting the
    /// performance mode. These settings only exist for the AC Custom profile.
    pub fn set_custom_mode_config(&mut self, cpu_level: u8, gpu_level: u8) -> AppResult<()> {
        validate_custom_mode_level(cpu_level)?;
        validate_custom_mode_level(gpu_level)?;

        command(
            self.device,
            CUSTOM_MODE_CONFIG_COMMAND,
            &custom_mode_config_command_args(1, cpu_level),
            None,
        )?;
        self.app_config.custom_mode_config.cpu_level = cpu_level;

        command(
            self.device,
            CUSTOM_MODE_CONFIG_COMMAND,
            &custom_mode_config_command_args(2, gpu_level),
            None,
        )?;
        self.app_config.custom_mode_config.gpu_level = gpu_level;
        self.persist_config();
        Ok(())
    }

    /// Saves the fan speed for the active performance mode and reapplies that
    /// performance mode before updating all fan channels.
    #[allow(dead_code)]
    pub fn set_fan_speed(&mut self, speed: u8) -> AppResult<()> {
        validate_fan_speed(speed, self.fan_speed_limits)?;

        let perf_mode = self.app_config.get().perf_mode.value();
        self.app_config.get().fan_speeds.set(perf_mode, speed);
        self.set_perf_mode(perf_mode)
    }

    /// Returns the saved fan speed for the active performance mode.
    #[allow(dead_code)]
    pub fn get_fan_speed(&mut self) -> u8 {
        let state = self.app_config.get();
        let perf_mode = state.perf_mode.value();
        state.fan_speeds.get(perf_mode)
    }

    pub fn get_fan_speed_limits(&self) -> AppResult<FanSpeedLimits> {
        let result = command(self.device, 0x0008, &[0, 0, 0, 0], Some(&[1, 2]))?;
        let [max, min] = result.as_slice() else {
            return Err(AppError::Internal(
                "Fan-speed limit response did not include maximum and minimum values".to_string(),
            ));
        };
        if *min == 0 || min > max {
            return Err(AppError::Internal(format!(
                "Invalid fan-speed limits reported by firmware: min {min}, max {max}"
            )));
        }
        Ok(FanSpeedLimits {
            min: *min,
            max: *max,
        })
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

    fn apply_fan_speed(&self, speed: u8) -> AppResult<()> {
        validate_fan_speed(speed, self.fan_speed_limits)?;
        if speed == 0 {
            return Ok(());
        }
        for fan in 1..=4 {
            command(
                self.device,
                0x0d01,
                &fan_speed_command_args(fan, speed),
                None,
            )?;
        }
        Ok(())
    }

    fn persist_config(&mut self) {
        persist_config(self.app_config, self.persist_buffer);
    }
}

