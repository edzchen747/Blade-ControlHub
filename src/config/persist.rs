use crate::core::shared_state::SCREEN_TARGET_LVL;
use crate::razer::config::AppConfig;
use crate::utils::persist::PersistBuffer;

use std::sync::atomic::Ordering;
use tracing::error;

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
