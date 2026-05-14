#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod razer;
mod ui;
mod utils;
mod win;

use crate::razer::device_handle::device;

fn main() {
    let exe_path = std::env::current_exe().unwrap();
    let exe_dir = exe_path.parent().unwrap();
    std::env::set_current_dir(exe_dir).unwrap();

    utils::reload::close_running_instances();
    start_razer_service();
    ui::app::run();
}

fn start_razer_service() {
    let device_pid = razer::device_handle::device().get_pid();
    win::input::start_keyboard_hooks(device_pid);
    win::system::power::PowerMonitor::start();
    win::system::standby::StandbyMonitor::start();
    win::external_events::ExternalChangeMonitor::start();
    println!("Razer service started...");
    device().initialize(true);
}
