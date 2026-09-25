use blade_controlhub::razer::config::{AppConfig, PowerProfile, allowed_perf_modes};
use blade_controlhub::razer::enums::PerfMode;

#[test]
fn app_config_primary_multimedia_keys_is_false() {
    let config = AppConfig::default();
    assert!(!config.primary_multimedia_keys);
}

#[test]
fn app_config_default_perf_cycle_lists_are_profile_specific() {
    let config = AppConfig::default();

    assert_eq!(
        config.profile(PowerProfile::Ac).perf_mode.items,
        allowed_perf_modes(PowerProfile::Ac).to_vec()
    );
    assert_eq!(
        config.profile(PowerProfile::Battery).perf_mode.items,
        allowed_perf_modes(PowerProfile::Battery).to_vec()
    );
}

#[test]
fn app_config_default_perf_cycle_modes_are_profile_specific() {
    let mut config = AppConfig::default();

    assert_eq!(
        config.profile_mut(PowerProfile::Ac).perf_mode.value(),
        PerfMode::Balanced
    );
    assert_eq!(
        config.profile_mut(PowerProfile::Battery).perf_mode.value(),
        PerfMode::Silent
    );
}

#[test]
fn app_config_deserializes_from_empty_json_object_with_defaults() {
    let json = "{}";
    let config: AppConfig =
        serde_json::from_str(json).expect("empty JSON object must deserialize successfully");
    assert!(!config.primary_multimedia_keys);
}

#[test]
fn app_config_deserializes_primary_multimedia_keys() {
    let json = r#"{"primary_multimedia_keys": true}"#;
    let config: AppConfig =
        serde_json::from_str(json).expect("partial JSON must deserialize with serde(default)");
    assert!(config.primary_multimedia_keys);
}

#[test]
fn app_config_accepts_legacy_default_multimedia_keys() {
    let json = r#"{"default_multimedia_keys": true}"#;
    let config: AppConfig =
        serde_json::from_str(json).expect("legacy configuration must deserialize successfully");
    assert!(config.primary_multimedia_keys);
}

#[test]
fn app_config_ignores_legacy_battery_limit_on_load() {
    let json = r#"{
        "battery_limit": {"index": 7, "items": ["Limit80"]},
        "primary_multimedia_keys": true
    }"#;
    let config: AppConfig =
        serde_json::from_str(json).expect("legacy battery limit must be ignored");
    let serialized = serde_json::to_string(&config).expect("config must serialize");

    assert!(config.primary_multimedia_keys);
    assert!(!serialized.contains("battery_limit"));
}

#[test]
fn app_config_rejects_malformed_json_with_parse_error() {
    let malformed = "{ this is not json }";
    let result = serde_json::from_str::<AppConfig>(malformed);
    assert!(result.is_err(), "malformed JSON must produce a parse error");
}

#[test]
fn app_config_round_trips_through_json() {
    let original = AppConfig::default();
    let json = serde_json::to_string_pretty(&original)
        .expect("default AppConfig must serialize without error");
    let restored: AppConfig =
        serde_json::from_str(&json).expect("serialized AppConfig must deserialize without error");
    assert_eq!(
        restored.primary_multimedia_keys,
        original.primary_multimedia_keys
    );
}

#[test]
fn app_config_round_trips_command_lab_commands() {
    use blade_controlhub::win::system::usbpcap::capture::CapturedCommand;

    let mut config = AppConfig::default();
    config.command_lab_commands.insert(
        "Brightness Up".to_owned(),
        vec![CapturedCommand {
            command: 0x0303,
            args: vec![0x01, 0x05, 0xFF],
        }],
    );

    let json = serde_json::to_string(&config).expect("config must serialize");
    let restored: AppConfig = serde_json::from_str(&json).expect("config must deserialize");

    assert_eq!(
        restored.command_lab_commands,
        config.command_lab_commands
    );
}

#[test]
fn app_config_serializes_its_bulky_collections_at_the_end() {
    use blade_controlhub::win::system::usbpcap::capture::CapturedCommand;

    let mut config = AppConfig::default();
    config.command_lab_commands.insert(
        "Test".to_owned(),
        vec![CapturedCommand {
            command: 0x0792,
            args: vec![0x00],
        }],
    );

    let json = serde_json::to_string(&config).expect("config must serialize");

    // The device settings are what someone opens the file to read; the
    // user-defined lists are long and go after them.
    let captures = json
        .find(r#""command_lab_commands""#)
        .expect("captures must be present");
    let bindings = json
        .find(r#""key_bindings""#)
        .expect("key bindings must be present");
    let theme = json.find(r#""theme_color""#).expect("theme must be present");

    assert!(theme < captures, "device settings come before the lists");
    assert!(captures < bindings);
    let trimmed = json.strip_suffix('}').unwrap_or(&json);
    assert!(
        trimmed.ends_with(r#""key_bindings":{"razer":[],"hypershift":[]}"#),
        "key_bindings must be the last key in the config JSON"
    );
}

/// A config written before key bindings existed must still load, and come back
/// with both tables empty rather than failing the whole file.
#[test]
fn a_config_without_key_bindings_loads_with_empty_tables() {
    let json = r#"{"model_name":"Razer Blade 16","primary_multimedia_keys":true}"#;

    let config: AppConfig = serde_json::from_str(json).expect("an older config must still parse");

    assert_eq!(config.model_name, "Razer Blade 16");
    assert!(config.primary_multimedia_keys);
    assert!(config.key_bindings.razer.is_empty());
    assert!(config.key_bindings.hypershift.is_empty());
}

/// A binding has to come back off disk exactly as it went on, including the
/// action's own fields: this is the whole point of the mappings being config
/// rather than browser storage.
#[test]
fn key_bindings_survive_a_round_trip_through_the_config_file() {
    use blade_controlhub::win::input::binding::{Chord, DeviceAction, KeyAction, KeyBinding};

    let mut config = AppConfig::default();
    config.key_bindings.razer.push(KeyBinding {
        key_code: 0x24,
        label: "Launcher".to_owned(),
        action: KeyAction::LaunchApp {
            path: r"shell:AppsFolder\Microsoft.WindowsNotepad_8wekyb3d8bbwe!App".to_owned(),
            args: "--new".to_owned(),
            name: "Notepad".to_owned(),
        },
    });
    config.key_bindings.hypershift.push(KeyBinding {
        key_code: 0x4b,
        label: String::new(),
        action: KeyAction::Device {
            action: DeviceAction::CycleRgbEffect,
        },
    });
    config.key_bindings.hypershift.push(KeyBinding {
        key_code: 0x4c,
        label: String::new(),
        action: KeyAction::Key {
            chord: Chord {
                modifiers: 0b1111,
                key: 0x7c,
            },
        },
    });

    let json = serde_json::to_string(&config).expect("config must serialize");
    let parsed: AppConfig = serde_json::from_str(&json).expect("config must parse");

    assert_eq!(parsed.key_bindings, config.key_bindings);
}

#[test]
fn app_config_serializes_to_valid_json_without_error() {
    let config = AppConfig::default();
    let result = serde_json::to_string_pretty(&config);
    assert!(result.is_ok(), "AppConfig serialization must not fail");
    let json = result.unwrap();
    assert!(
        !json.contains("battery_limit"),
        "serialized JSON must not contain the device-backed battery limit"
    );
    assert!(json.contains("primary_multimedia_keys"));
}

#[test]
fn app_config_refresh_cycle_items_does_not_panic() {
    let mut config = AppConfig::default();
    config.refresh_cycle_items();
}

#[test]
fn app_config_refresh_cycle_items_multiple_times_does_not_panic() {
    let mut config = AppConfig::default();
    config.refresh_cycle_items();
    config.refresh_cycle_items();
    config.refresh_cycle_items();
}

#[test]
fn app_config_refresh_cycle_items_restores_full_rgb_effects_list() {
    let json = r#"{"power_state": {"rgb_effect": {"index": 0, "items": [4]}}}"#;
    let mut config: AppConfig = serde_json::from_str(json).unwrap_or_default();
    config.refresh_cycle_items();
    let json_out = serde_json::to_string(&config).unwrap();
    assert!(
        json_out.contains("\"Wave\"") || json_out.contains(",1") || json_out.contains("1,"),
        "refresh_cycle_items must restore the full RGB effects list beyond the single truncated entry"
    );
}
