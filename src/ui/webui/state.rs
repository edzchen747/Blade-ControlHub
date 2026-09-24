//! The snapshot the settings window renders from.
//!
//! [`UiState`] is [`SettingsState`] plus the presentation facts the window
//! would otherwise have to hard-code: the human label for each enum variant,
//! the performance-mode colours and which modes each power profile allows.
//! Keeping them here means `strum`'s `Display` and `allowed_perf_modes` stay
//! the single source of truth for both the OSD and the window.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::razer::config::{PowerProfile, allowed_perf_modes};
use crate::razer::enums::{BATTERY_LIMITS, BatteryLimit, PERF_MODES, PerfMode, RGBEffect};
use crate::runtime::settings_state::SettingsState;
use crate::ui::theme::perf_mode_hex_color;

#[derive(Clone, Debug, Serialize)]
pub struct UiState {
    #[serde(flatten)]
    pub settings: SettingsState,
    pub meta: UiMeta,
}

/// Static per-device presentation data. It only changes when the device or the
/// experimental-features flag does, but it travels with every snapshot so the
/// window has exactly one input to render from.
#[derive(Clone, Debug, Serialize)]
pub struct UiMeta {
    pub perf_mode_labels: BTreeMap<String, String>,
    pub perf_mode_colors: BTreeMap<String, String>,
    pub rgb_effect_labels: BTreeMap<String, String>,
    pub battery_limit_labels: BTreeMap<String, String>,
    pub battery_limit_percents: BTreeMap<String, Option<u8>>,
    pub allowed_perf_modes: ProfileModes,
    pub custom_mode_levels: Vec<String>,
    pub keyboard_brightness_step: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProfileModes {
    pub ac: Vec<PerfMode>,
    pub battery: Vec<PerfMode>,
}

/// One keyboard backlight stop. The firmware exposes 0..=255 but only five
/// distinct levels, so the window renders five dots rather than a slider.
pub const KEYBOARD_BRIGHTNESS_STEP: u8 = 51;

impl UiState {
    pub fn new(settings: SettingsState) -> Self {
        Self {
            settings,
            meta: UiMeta::current(),
        }
    }
}

impl UiMeta {
    pub fn current() -> Self {
        Self {
            perf_mode_labels: PERF_MODES
                .iter()
                .chain([&PerfMode::Unsupported, &PerfMode::Unknown])
                .map(|mode| (variant_key(mode), mode.to_string()))
                .collect(),
            perf_mode_colors: PERF_MODES
                .iter()
                .chain([&PerfMode::Unsupported, &PerfMode::Unknown])
                .map(|mode| (variant_key(mode), perf_mode_hex_color(*mode).to_owned()))
                .collect(),
            rgb_effect_labels: crate::razer::enums::RGB_EFFECTS
                .iter()
                .chain([&RGBEffect::Unknown])
                .map(|effect| (variant_key(effect), effect.to_string()))
                .collect(),
            battery_limit_labels: BATTERY_LIMITS
                .iter()
                .map(|limit| (variant_key(limit), battery_limit_label(*limit)))
                .collect(),
            battery_limit_percents: BATTERY_LIMITS
                .iter()
                .map(|limit| (variant_key(limit), battery_limit_percent(*limit)))
                .collect(),
            allowed_perf_modes: ProfileModes {
                ac: allowed_perf_modes(PowerProfile::Ac),
                battery: allowed_perf_modes(PowerProfile::Battery),
            },
            custom_mode_levels: ["Low", "Medium", "High", "Max"]
                .iter()
                .map(|level| (*level).to_owned())
                .collect(),
            keyboard_brightness_step: KEYBOARD_BRIGHTNESS_STEP,
        }
    }
}

/// The key the window sees for an enum variant: the same string serde writes,
/// so a label lookup and a command argument always agree.
fn variant_key<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// `BatteryLimit`'s `Display` is prefixed for the OSD ("Limit: 65%"); the
/// window already labels the control, so it shows the bare value.
fn battery_limit_label(limit: BatteryLimit) -> String {
    limit.to_string().replace("Limit: ", "")
}

fn battery_limit_percent(limit: BatteryLimit) -> Option<u8> {
    match limit {
        BatteryLimit::Off | BatteryLimit::Unknown => None,
        BatteryLimit::Limit50 => Some(50),
        BatteryLimit::Limit55 => Some(55),
        BatteryLimit::Limit60 => Some(60),
        BatteryLimit::Limit65 => Some(65),
        BatteryLimit::Limit70 => Some(70),
        BatteryLimit::Limit75 => Some(75),
        BatteryLimit::Limit80 => Some(80),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_keys_match_the_serialized_command_arguments() {
        assert_eq!(variant_key(&PerfMode::Turbo), "Turbo");
        assert_eq!(variant_key(&RGBEffect::AudioBloom), "AudioBloom");
        assert_eq!(variant_key(&BatteryLimit::Limit65), "Limit65");
    }

    #[test]
    fn every_listed_perf_mode_carries_a_label_and_a_colour() {
        let meta = UiMeta::current();
        for mode in PERF_MODES {
            let key = variant_key(&mode);
            assert!(meta.perf_mode_labels.contains_key(&key), "label for {key}");
            assert!(meta.perf_mode_colors.contains_key(&key), "colour for {key}");
        }
    }

    #[test]
    fn battery_limit_labels_drop_the_osd_prefix() {
        let meta = UiMeta::current();
        assert_eq!(meta.battery_limit_labels["Limit65"], "65%");
        assert_eq!(meta.battery_limit_labels["Off"], "Off");
        assert_eq!(meta.battery_limit_percents["Limit65"], Some(65));
        assert_eq!(meta.battery_limit_percents["Off"], None);
    }

    #[test]
    fn custom_perf_mode_is_offered_on_ac_only() {
        let meta = UiMeta::current();
        assert!(meta.allowed_perf_modes.ac.contains(&PerfMode::Custom));
        assert!(!meta.allowed_perf_modes.battery.contains(&PerfMode::Custom));
    }
}
