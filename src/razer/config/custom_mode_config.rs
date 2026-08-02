use serde::{Deserialize, Serialize};

/// AC-only CPU and GPU limits used while the Custom performance mode is active.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomModeConfig {
    pub cpu_level: u8,
    pub gpu_level: u8,
}
