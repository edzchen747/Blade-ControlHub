use crate::core::shared_state::SCREEN_TARGET_LVL;
use crate::razer::config::AppConfig;
use crate::utils::persist::PersistBuffer;

use super::CONFIG_PATH;
use std::sync::atomic::Ordering;
use tracing::error;

/// Persists the current application config to disk via the provided buffer.
///
/// Snapshots the current screen brightness into the config before writing.
pub fn persist_config(app_config: &mut AppConfig, persist_buffer: &PersistBuffer) {
    app_config.get().screen_lvl = SCREEN_TARGET_LVL.load(Ordering::SeqCst);

    let json = match serde_json::to_string_pretty(&*app_config) {
        Ok(json) => json,
        Err(e) => {
            error!(error = %e, "Config serialization failed; skipping disk write");
            return;
        }
    };
    persist_buffer.write(json);
}

pub fn persist_config_now(app_config: &AppConfig) {
    let json = match serde_json::to_string_pretty(app_config) {
        Ok(json) => json,
        Err(e) => {
            error!(error = %e, "Config serialization failed; skipping immediate disk write");
            return;
        }
    };
    if let Err(e) = std::fs::write(CONFIG_PATH, json) {
        error!(error = %e, "Failed to write config immediately");
    }
}
