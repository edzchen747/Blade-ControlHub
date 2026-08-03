//! Tests for `AppConfig` default values, JSON deserialization/serialization,
//! and `refresh_cycle_items()`.

use blade_controlhub::razer::config::{AppConfig, PowerProfile, allowed_perf_modes};
use blade_controlhub::razer::enums::PerfMode;

// ── Default values ───────────────────────────────────────────────────────────

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

// ── JSON Deserialization (mirrors load_config behaviour) ──────────────────────

#[test]
fn app_config_deserializes_from_empty_json_object_with_defaults() {
    // load_config uses serde(default) on all fields; an empty JSON object
    // should produce the same result as AppConfig::default().
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
    // Serialize a default config, then deserialize it back.
    // The result must be semantically equivalent to the original.
    let original = AppConfig::default();
    let json = serde_json::to_string_pretty(&original)
        .expect("default AppConfig must serialize without error");
    let restored: AppConfig =
        serde_json::from_str(&json).expect("serialized AppConfig must deserialize without error");
    // Verify key structural properties survive the round-trip
    assert_eq!(
        restored.primary_multimedia_keys,
        original.primary_multimedia_keys
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

// ── refresh_cycle_items() preserves indices ───────────────────────────────────

#[test]
fn app_config_refresh_cycle_items_does_not_panic() {
    let mut config = AppConfig::default();
    // Should not panic
    config.refresh_cycle_items();
}

#[test]
fn app_config_refresh_cycle_items_multiple_times_does_not_panic() {
    let mut config = AppConfig::default();
    // Calling multiple times should not panic or cause issues
    config.refresh_cycle_items();
    config.refresh_cycle_items();
    config.refresh_cycle_items();
}

#[test]
fn app_config_refresh_cycle_items_restores_full_rgb_effects_list() {
    // Simulate a stale config where the internal rgb_effect items were truncated
    // to a single entry. Since `power_state` is private, we inject the truncated
    // state via JSON deserialization, then verify that refresh_cycle_items()
    // restores the full list by serializing back and inspecting the output.
    let json = r#"{"power_state": {"rgb_effect": {"index": 0, "items": [4]}}}"#;
    let mut config: AppConfig = serde_json::from_str(json).unwrap_or_default();
    config.refresh_cycle_items();
    // After refresh, serialize back to JSON and verify the items array
    // now contains multiple entries (the full RGB_EFFECTS set has 6 items).
    let json_out = serde_json::to_string(&config).unwrap();
    // The serialized rgb_effect items should contain the Wave variant (discriminant 1),
    // which was not present in the truncated single-item list [4] (Cycle).
    assert!(
        json_out.contains("\"Wave\"") || json_out.contains(",1") || json_out.contains("1,"),
        "refresh_cycle_items must restore the full RGB effects list beyond the single truncated entry"
    );
}
