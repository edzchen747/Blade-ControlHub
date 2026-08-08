use std::sync::atomic::Ordering;

use crate::core::shared_state::PRIMARY_MULTIMEDIA_KEYS;
use crate::razer::config::AppConfig;

use super::constants::CONFIG_PATH;
use librazer::descriptor::Descriptor;
use tracing::{info, warn};

/// Loads the application config from disk, falling back to defaults on error.
///
/// Automatically handles backwards compatibility by using `#[serde(default)]`
/// on all fields, so missing or unknown keys are gracefully ignored.
pub fn load_config(device_info: &Descriptor) -> AppConfig {
    let Ok(contents) = std::fs::read_to_string(CONFIG_PATH) else {
        info!("Config file not found, using application defaults");
        return finalize_config(AppConfig::default(), device_info);
    };

    if contents.trim().is_empty() {
        return finalize_config(AppConfig::default(), device_info);
    }

    let app_config: AppConfig = match serde_json::from_str(&contents) {
        Ok(config) => {
            info!("Config loaded successfully");
            config
        }
        Err(e) => {
            warn!(error = %e, "Config parse failed, reverting to defaults");
            AppConfig::default()
        }
    };

    finalize_config(app_config, device_info)
}

pub fn load_launch_flags() -> (bool, bool) {
    let Ok(contents) = std::fs::read_to_string(CONFIG_PATH) else {
        return (false, false);
    };
    load_launch_flags_from_contents(&contents)
}

fn load_launch_flags_from_contents(contents: &str) -> (bool, bool) {
    match serde_json::from_str::<AppConfig>(contents) {
        Ok(config) => (config.start_with_admin, config.start_with_windows),
        Err(error) => {
            warn!(%error, "Failed to parse config for launch flags; using defaults");
            (false, false)
        }
    }
}

fn finalize_config(mut app_config: AppConfig, device_info: &Descriptor) -> AppConfig {
    // Override saved cycle items in case new updates bring more options
    app_config.refresh_cycle_items();
    app_config.set_device_model(
        format!("0x{:04x}", device_info.pid),
        device_info.name.to_string(),
    );

    PRIMARY_MULTIMEDIA_KEYS.store(app_config.primary_multimedia_keys, Ordering::SeqCst);

    app_config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_descriptor() -> Descriptor {
        Descriptor {
            model_number_prefix: "RZ09",
            name: "Razer Blade Test",
            pid: 0x02c7,
            features: &[],
        }
    }

    #[test]
    fn finalize_config_overrides_saved_device_model_from_descriptor() {
        let json = r#"{
            "model_pid": "0x0000",
            "model_name": "Stale Model",
            "primary_multimedia_keys": true
        }"#;
        let config: AppConfig = serde_json::from_str(json).expect("test JSON must parse");

        let finalized = finalize_config(config, &test_descriptor());
        let serialized = serde_json::to_value(&finalized).expect("config must serialize");

        assert_eq!(serialized["model_pid"], "0x02c7");
        assert_eq!(finalized.model_name, "Razer Blade Test");
        assert!(finalized.primary_multimedia_keys);
    }

    #[test]
    fn finalize_config_adds_device_model_to_default_config() {
        let finalized = finalize_config(AppConfig::default(), &test_descriptor());
        let serialized = serde_json::to_value(&finalized).expect("config must serialize");

        assert_eq!(serialized["model_pid"], "0x02c7");
        assert_eq!(finalized.model_name, "Razer Blade Test");
    }

    #[test]
    fn launch_flags_default_to_disabled_without_config_file() {
        let (start_with_admin, start_with_windows) = load_launch_flags();

        assert!(!start_with_admin);
        assert!(!start_with_windows);
    }

    #[test]
    fn launch_flags_are_read_from_the_persisted_config() {
        let json = serde_json::json!({
            "start_with_admin": true,
            "start_with_windows": true,
        })
        .to_string();

        assert_eq!(load_launch_flags_from_contents(&json), (true, true));
    }

    #[test]
    fn corrupt_config_falls_back_to_disabled_launch_flags() {
        assert_eq!(load_launch_flags_from_contents("{not json"), (false, false));
    }
}
