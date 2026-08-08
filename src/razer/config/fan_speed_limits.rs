use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanSpeedLimits {
    pub min: u8,
    pub max: u8,
}

impl Default for FanSpeedLimits {
    fn default() -> Self {
        Self { min: 10, max: 46 }
    }
}
impl FanSpeedLimits {
    pub fn contains(self, speed: u8) -> bool {
        (self.min..=self.max).contains(&speed)
    }
    pub fn midpoint(self) -> u8 {
        self.min + (self.max - self.min) / 2
    }
}
