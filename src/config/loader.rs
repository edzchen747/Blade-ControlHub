use std::sync::atomic::Ordering;

use crate::core::shared_state::PRIMARY_MULTIMEDIA_KEYS;
use crate::razer::config::AppConfig;

use super::constants::CONFIG_PATH;
use librazer::descriptor::Descriptor;
use tracing::{info, warn};

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

fn finalize_config(mut app_config: AppConfig, device_info: &Descriptor) -> AppConfig {
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
}
