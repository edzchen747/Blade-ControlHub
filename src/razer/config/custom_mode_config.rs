use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomModeConfig {
    pub cpu_level: u8,
    pub gpu_level: u8,
}
