#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod razer;
mod ui;
mod win;

use std::{thread, time::Duration};
use sysinfo::System;

fn main() {
    close_running_instances();
    start_razer_service();
    println!("Razer service started...");
    ui::tray_app::run();
}

fn start_razer_service() {
    let device_pid = razer::device_handle::device().get_pid();
    win::key_hook::init_keyboard_hooks(device_pid);
    win::power::PowerMonitor::new(); // start power monitor before intialize so we know which power state config to intialise
    razer::device_handle::device().initialize();
    win::standby::spawn_listener_thread();
    win::external_events::spawn_detect_external_updates_thread();
}

fn close_running_instances() {
    let mut sys = System::new_all();
    sys.refresh_all();

    let current_pid = sysinfo::get_current_pid().expect("Failed to get app PID");
    let my_name = "hidapi.exe";

    let mut found_old = false;
    for (pid, process) in sys.processes() {
        if process.name() == my_name && *pid != current_pid {
            process.kill();
            found_old = true;
        }
    }

    if found_old {
        thread::sleep(Duration::from_millis(150));
    }
}
