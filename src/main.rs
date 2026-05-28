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

use crate::{
    error::AppResult,
    razer::device_handle::device,
    utils::log_file::{init_log_file_writer, set_cwd},
};
use tracing::info;

fn main() -> AppResult<()> {
    set_cwd();
    init_log_file_writer();

    utils::reload::close_running_instances();
    start_razer_service()?;
    ui::app::run();
    Ok(())
}

fn start_razer_service() -> AppResult<()> {
    let device_pid = razer::device_handle::device().get_pid().unwrap();
    info!("Detected device with PID: 0x{:04x}", device_pid);
    win::input::start_keyboard_hooks(device_pid)?;
    win::system::power::PowerMonitor::start();
    win::system::standby::StandbyMonitor::start();
    win::external_events::ExternalChangeMonitor::start();
    win::system::display_gpu::GpuDisplayMonitor::start();
    info!("Razer service started");
    let notify_startup = !std::env::args().any(|arg| arg == "--silent");
    device().initialize(notify_startup);
    Ok(())
}
