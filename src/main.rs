mod razer;
mod ui;
mod win;

fn main() {
    let mut tray = ui::tray_icon::TrayIcon::new();
    start_razer_service();
    println!("Razer service started...");
    tray.run();
}

fn start_razer_service() {
    let device_pid = razer::device_handle::device().get_pid();
    win::key_hook::init_keyboard_hooks(device_pid);
    win::power::PowerMonitor::new(); // start power monitor before intialize so we know which power state config to intialise
    razer::device_handle::device().initialize();
    win::standby::spawn_listener_thread();
    win::external_events::spawn_detect_external_updates_thread();
}
