#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use blade_controlhub::{
    error::AppResult,
    razer::{self, device_handle::device},
    ui,
    ui::app_events::{OsdEvent, OsdIcon},
    ui::osd_controller::{OsdController, OsdParams},
    utils,
    utils::log_file::{init_log_file_writer, set_cwd},
    win,
};
use tracing::info;

fn main() -> AppResult<()> {
    set_cwd()?;
    init_log_file_writer();

    if std::env::args().any(|arg| arg == "--mock-trigger") {
        run_mock_trigger();
        return Ok(());
    }

    utils::reload::close_running_instances();
    ui::app::init();
    start_razer_service()?;
    ui::app::run();
    Ok(())
}

fn start_razer_service() -> AppResult<()> {
    let device_pid = razer::device_handle::device().get_pid()?;
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

fn run_mock_trigger() {
    for step in 0..10 {
        if step % 2 == 0 {
            OsdController::show(OsdParams {
                label: "Mock OSD".to_string(),
                total_steps: 0,
                active_steps: 0,
                icon: Some(OsdIcon::RazerControlHub),
            });
        } else if let Some(params) = OsdEvent::ScreenBrightness(70).as_params() {
            OsdController::show(params);
        }

        std::thread::sleep(std::time::Duration::from_millis(700));
    }

    std::thread::sleep(std::time::Duration::from_secs(2));
}
