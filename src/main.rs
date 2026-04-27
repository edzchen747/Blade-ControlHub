mod win;
mod razer;

// use std::time::Duration;
// use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let device_pid = razer::device_handle::device().get_pid();

    win::key_hook::init_keyboard_hooks(device_pid)?;

    let _ = razer::device_handle::device().initialize_keyboard();

    win::power::spawn_listener_thread()
}

// fn get_time_string() -> String {
//     let now = SystemTime::now()
//         .duration_since(UNIX_EPOCH)
//         .unwrap_or_default();
    
//     let total_secs = now.as_secs();
    
//     let sec = total_secs % 60;
//     let min = (total_secs / 60) % 60;
//     let hour = (total_secs / 3600) % 24;

//     // Formats with leading zeros (e.g., 09:05:01)
//     format!("{:02}:{:02}:{:02}", hour, min, sec)
// }
