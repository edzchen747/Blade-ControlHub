//! The snapshot the settings window renders from.
//!
//! [`UiState`] is [`SettingsState`] plus the presentation facts the window
//! would otherwise have to hard-code: the human label for each enum variant,
//! the performance-mode colours and which modes each power profile allows.
//! Keeping them here means `strum`'s `Display` and `allowed_perf_modes` stay
//! the single source of truth for both the OSD and the window.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::core::capabilities;
use crate::razer::config::{PowerProfile, allowed_perf_modes};
use crate::razer::enums::{BATTERY_LIMITS, BatteryLimit, PERF_MODES, PerfMode, RGBEffect};
use crate::runtime::settings_state::SettingsState;
use crate::ui::theme::perf_mode_hex_color;
use crate::win::input::binding::available_device_actions;
use crate::win::input::builtin_meta;

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
    /// Whether this model has the illuminated vent under the chassis. Only the
    /// Blade 18 does, and the Lighting page hides its toggle without one.
    pub vapour_chamber: bool,
    /// The device actions a key can be bound to, so the Keys page never has to
    /// hard-code the vocabulary the runtime accepts.
    pub device_action_labels: BTreeMap<String, String>,
    /// What each Fn-layer key does before the user rebinds it, for the
    /// override hint. Razer's special keys have no built-in action to
    /// override, so there is nothing to describe for them.
    pub built_in_hypershift: BTreeMap<u16, String>,
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
            vapour_chamber: capabilities::has_vapour_chamber(),
            device_action_labels: available_device_actions()
                .iter()
                .map(|action| (variant_key(action), action.label().to_owned()))
                .collect(),
            built_in_hypershift: builtin_meta::hypershift_labels(),
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

    /// The window mirrors this shape by hand in `ui/src/lib/types.ts`, and it
    /// changed when the Razer built-ins went away, so the keys it reads are
    /// pinned here rather than left to the derive.
    #[test]
    fn the_snapshot_describes_the_fn_layer_and_nothing_else() {
        let meta = capabilities::with_vapour_chamber(true, UiMeta::current);
        let json = serde_json::to_value(&meta).expect("meta must serialize");

        assert!(
            json.get("built_in_hypershift").is_some(),
            "the Fn layer still has stock actions to describe"
        );
        assert!(
            json.get("built_in_bindings").is_none(),
            "Razer special keys have no built-in action to override"
        );
        assert_eq!(json["vapour_chamber"], serde_json::json!(true));
    }

    /// Everything the Lighting and Keys pages gate on the vapour chamber comes
    /// from this one snapshot, so all three answers have to move together.
    #[test]
    fn the_snapshot_hides_every_vapour_chamber_control_without_the_hardware() {
        let present = capabilities::with_vapour_chamber(true, UiMeta::current);
        assert!(present.vapour_chamber);
        assert!(present.device_action_labels.contains_key("toggle_underglow"));
        assert!(present.built_in_hypershift.contains_key(&0x56));

        let absent = capabilities::with_vapour_chamber(false, UiMeta::current);
        assert!(!absent.vapour_chamber);
        assert!(
            !absent.device_action_labels.contains_key("toggle_underglow"),
            "a key must not be bindable to a light this model has not got"
        );
        assert!(
            !absent.built_in_hypershift.contains_key(&0x56),
            "Fn+V overrides nothing where it does nothing"
        );
        assert!(
            absent.device_action_labels.contains_key("cycle_perf_mode"),
            "the rest of the vocabulary is unaffected"
        );
        assert!(
            absent.built_in_hypershift.contains_key(&0x54),
            "the rest of the Fn layer is unaffected"
        );
    }

    #[test]
    fn custom_perf_mode_is_offered_on_ac_only() {
        let meta = UiMeta::current();
        assert!(meta.allowed_perf_modes.ac.contains(&PerfMode::Custom));
        assert!(!meta.allowed_perf_modes.battery.contains(&PerfMode::Custom));
    }
}
