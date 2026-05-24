#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod core;
mod error;
mod razer;
#[cfg(test)]
mod tests;
mod ui;
mod utils;
mod win;

use crate::{error::AppResult, razer::device_handle::device};
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() -> AppResult<()> {
    let builder = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug")),
        )
        .with_target(false)
        .compact();

    let _guard;

    if cfg!(debug_assertions) {
        builder.init();
    } else {
        let file_appender = tracing_appender::rolling::never(".", "app.log");

        let (file_writer, g) = tracing_appender::non_blocking(file_appender);
        _guard = Some(g);

        builder.with_writer(file_writer).init();
    }

    let exe_path = std::env::current_exe().map_err(crate::error::AppError::Io)?;
    let exe_dir = exe_path.parent().ok_or_else(|| {
        crate::error::AppError::Internal("Executable has no parent directory".to_string())
    })?;
    std::env::set_current_dir(exe_dir).map_err(crate::error::AppError::Io)?;

    utils::reload::close_running_instances();
    start_razer_service()?;
    ui::app::run();
    Ok(())
}

fn start_razer_service() -> AppResult<()> {
    // Returns 0 if the device thread has not yet started or has exited;
    // a PID of 0 causes the HID listener to skip product-ID filtering.
    let device_pid = razer::device_handle::device().get_pid().unwrap_or(0);
    info!("Detected device with PID: 0x{:04x}", device_pid);
    win::input::start_keyboard_hooks(device_pid)?;
    win::system::power::PowerMonitor::start();
    win::system::standby::StandbyMonitor::start();
    win::external_events::ExternalChangeMonitor::start();
    info!("Razer service started");
    let notify_startup = !std::env::args().any(|arg| arg == "--silent");
    device().initialize(notify_startup);
    Ok(())
}
