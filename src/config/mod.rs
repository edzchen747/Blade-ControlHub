use crate::razer::config::AppConfig;
use crate::utils::persist::PersistBuffer;
use crate::win::display::brightness::SCREEN_TARGET_LVL;

use std::sync::atomic::Ordering;

// ── Application Constants ───────────────────────────────────────────────────

/// Razer USB Vendor ID — non-configurable, this app is designed for Razer devices.
pub const RAZER_VID: u16 = 0x1532;

/// Path to the persisted configuration file on disk.
pub const CONFIG_PATH: &str = "config.json";

// ── Configuration Loading ───────────────────────────────────────────────────

/// Loads the application config from disk, falling back to defaults on error.
///
/// Automatically handles backwards compatibility by using `#[serde(default)]`
/// on all fields, so missing or unknown keys are gracefully ignored.
pub fn load_config() -> AppConfig {
    let Ok(contents) = std::fs::read_to_string(CONFIG_PATH) else {
        println!("Config not found, using defaults.");
        return AppConfig::default();
    };

    if contents.trim().is_empty() {
        return AppConfig::default();
    }

    let mut app_config: AppConfig = match serde_json::from_str(&contents) {
        Ok(config) => {
            println!("Config loaded successfully.");
            config
        }
        Err(e) => {
            println!("Failed to parse config: {}. Using defaults.", e);
            AppConfig::default()
        }
    };

    // Override saved cycle items in case new updates bring more options
    app_config.refresh_cycle_items();

    app_config
}

// ── Configuration Persistence ───────────────────────────────────────────────

/// Persists the current application config to disk via the provided buffer.
///
/// Snapshots the current screen brightness into the config before writing.
pub fn persist_config(app_config: &mut AppConfig, persist_buffer: &PersistBuffer) {
    app_config.get().screen_lvl = SCREEN_TARGET_LVL.load(Ordering::SeqCst);

    match serde_json::to_string_pretty(&*app_config) {
        Ok(json) => {
            let _ = persist_buffer.write(json);
        }
        Err(e) => {
            eprintln!("Failed to serialize config for persistence: {}", e);
        }
    }
}
