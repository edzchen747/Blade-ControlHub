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

fn main() -> AppResult<()> {
    let exe_path = std::env::current_exe().unwrap();
    let exe_dir = exe_path.parent().unwrap();
    std::env::set_current_dir(exe_dir).unwrap();

    utils::reload::close_running_instances();
    start_razer_service()?;
    ui::app::run();
    Ok(())
}

fn start_razer_service() -> AppResult<()> {
    let device_pid = razer::device_handle::device().get_pid();
    win::input::start_keyboard_hooks(device_pid);
    win::system::power::PowerMonitor::start();
    win::system::standby::StandbyMonitor::start();
    win::external_events::ExternalChangeMonitor::start();
    println!("Razer service started...");
    device().initialize(true);
    Ok(())
}
