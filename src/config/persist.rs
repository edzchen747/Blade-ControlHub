use crate::razer::config::AppConfig;
use crate::utils::persist::PersistBuffer;
use crate::win::display::brightness::SCREEN_TARGET_LVL;

use std::sync::atomic::Ordering;

/// Persists the current application config to disk via the provided buffer.
///
/// Snapshots the current screen brightness into the config before writing.
pub fn persist_config(app_config: &mut AppConfig, persist_buffer: &PersistBuffer) {
    app_config.get().screen_lvl = SCREEN_TARGET_LVL.load(Ordering::SeqCst);

    let json = match serde_json::to_string_pretty(&*app_config) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("Failed to serialize config for persistence: {}", e);
            return;
        }
    };
    persist_buffer.write(json);
}
