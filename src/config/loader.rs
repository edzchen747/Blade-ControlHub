use crate::razer::config::AppConfig;

use super::constants::CONFIG_PATH;
use tracing::{info, warn};

/// Loads the application config from disk, falling back to defaults on error.
///
/// Automatically handles backwards compatibility by using `#[serde(default)]`
/// on all fields, so missing or unknown keys are gracefully ignored.
pub fn load_config() -> AppConfig {
    let Ok(contents) = std::fs::read_to_string(CONFIG_PATH) else {
        info!("Config file not found, using application defaults");
        return AppConfig::default();
    };

    if contents.trim().is_empty() {
        return AppConfig::default();
    }

    let mut app_config: AppConfig = match serde_json::from_str(&contents) {
        Ok(config) => {
            info!("Config loaded successfully");
            config
        }
        Err(e) => {
            warn!(error = %e, "Config parse failed, reverting to defaults");
            AppConfig::default()
        }
    };

    // Override saved cycle items in case new updates bring more options
    app_config.refresh_cycle_items();

    app_config
}
