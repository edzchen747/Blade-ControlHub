use serde::{Deserialize, Serialize};

use crate::razer::config::AppConfig;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsState {
    pub model_name: String,
    pub default_multimedia_keys: bool,
}

impl From<AppConfig> for SettingsState {
    fn from(config: AppConfig) -> Self {
        Self {
            model_name: config.model_name,
            default_multimedia_keys: config.default_multimedia_keys,
        }
    }
}
