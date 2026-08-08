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
fn app_config_serializes_command_lab_commands_at_the_end() {
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
    let trimmed = json.strip_suffix('}').unwrap_or(&json);
    assert!(
        trimmed.ends_with(r#""command_lab_commands":{"Test":[{"command":1938,"args":[0]}]}"#),
        "command_lab_commands must be the last key in the config JSON"
    );
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
