use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ThemeColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn to_hex_string(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    pub fn to_rgb_array(self) -> [f32; 3] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        ]
    }

    pub fn from_rgb_array(rgb: [f32; 3]) -> Self {
        Self {
            r: unit_to_u8(rgb[0]),
            g: unit_to_u8(rgb[1]),
            b: unit_to_u8(rgb[2]),
        }
    }
}

impl Default for ThemeColor {
    fn default() -> Self {
        Self::new(0xff, 0xd7, 0x00)
    }
}

fn unit_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_color_is_gold() {
        assert_eq!(ThemeColor::default().to_hex_string(), "#FFD700");
    }

    #[test]
    fn rgb_array_round_trips_to_bytes() {
        assert_eq!(
            ThemeColor::from_rgb_array([1.0, 128.0 / 255.0, 0.0]),
            ThemeColor::new(255, 128, 0)
        );
    }
}
