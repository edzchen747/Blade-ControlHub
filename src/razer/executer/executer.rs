use crate::config::ThemeColor;
use crate::config::persist_config;
use crate::core::shared_state::COMMAND_LAB_CAPTURE_ACTIVE;
use crate::disable_osd;
use crate::error::AppError;
use crate::razer::config::{AppConfig, FanSpeedLimits, PowerProfile};
use crate::razer::device_handle::DeviceCmd;
use crate::razer::enums::{BatteryLimit, PerfMode, RGBEffect};
use crate::razer::handlers::performance::{
    reset_perf_mode_for_sleep, validate_custom_mode_level, validate_fan_speed,
};
use crate::razer::handlers::{
    AudioHandler, BatteryHandler, DisplayHandler, KeyboardHandler, PerformanceHandler,
};
use crate::razer::protocol::command;
use crate::ui::app::app;
use crate::ui::app_events::OsdEvent;
use crate::utils::persist::PersistBuffer;
use crate::win::display::ambient::AmbientEffect;
use crate::win::display::brightness::BrightnessWorker;
use crate::win::display::refresh_rate::DisplayManager;
use librazer::device::Device;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};
use tracing::{debug, info, instrument, warn};

pub struct Executer<'a> {
    device: &'a mut Device,
    app_config: &'a mut AppConfig,
    persist_buffer: PersistBuffer,
    rx: Receiver<DeviceCmd>,
    urgent_rx: Receiver<DeviceCmd>,
    brightness_worker: BrightnessWorker,
    display_manager: DisplayManager,
    refresh_cycle_timeout: Instant,
    battery_cycle_timeout: Instant,
    settings_snapshot_queries: u32,
    cached_battery_limit: BatteryLimit,
    fan_speed_limits: FanSpeedLimits,
}

fn should_discard_during_capture(cmd: &DeviceCmd) -> bool {
    !matches!(cmd, DeviceCmd::Shutdown(_))
}
