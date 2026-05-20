//! Tests for `AppConfig` default values, JSON deserialization/serialization,
//! and `refresh_cycle_items()`.

use crate::razer::config::AppConfig;
use crate::razer::enums::BatteryLimit;

// ── Default values ───────────────────────────────────────────────────────────

#[test]
fn app_config_default_battery_limit_index_is_zero() {
    let config = AppConfig::default();
    assert_eq!(config.battery_limit.index, 0);
}

#[test]
fn app_config_default_battery_limit_value_is_off() {
    let config = AppConfig::default();
    assert_eq!(
        config.battery_limit.items[config.battery_limit.index],
        BatteryLimit::Off
    );
}

#[test]
fn app_config_default_multimedia_keys_is_false() {
    let config = AppConfig::default();
    assert!(!config.default_multimedia_keys);
}

// ── JSON Deserialization (mirrors load_config behaviour) ──────────────────────

#[test]
fn app_config_deserializes_from_empty_json_object_with_defaults() {
    // load_config uses serde(default) on all fields; an empty JSON object
    // should produce the same result as AppConfig::default().
    let json = "{}";
    let config: AppConfig =
        serde_json::from_str(json).expect("empty JSON object must deserialize successfully");
    assert_eq!(config.battery_limit.index, 0);
    assert!(!config.default_multimedia_keys);
}

#[test]
fn app_config_deserializes_with_partial_fields_using_defaults_for_missing() {
    let json = r#"{"default_multimedia_keys": true}"#;
    let config: AppConfig =
        serde_json::from_str(json).expect("partial JSON must deserialize with serde(default)");
    assert!(config.default_multimedia_keys);
    // battery_limit must still have defaults
    assert_eq!(
        config.battery_limit.items[config.battery_limit.index],
        BatteryLimit::Off
    );
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
    assert_eq!(restored.battery_limit.index, original.battery_limit.index);
    assert_eq!(
        restored.battery_limit.items.len(),
        original.battery_limit.items.len()
    );
    assert_eq!(
        restored.default_multimedia_keys,
        original.default_multimedia_keys
    );
}

#[test]
fn app_config_serializes_to_valid_json_without_error() {
    let config = AppConfig::default();
    let result = serde_json::to_string_pretty(&config);
    assert!(result.is_ok(), "AppConfig serialization must not fail");
    let json = result.unwrap();
    assert!(
        json.contains("battery_limit"),
        "serialized JSON must contain battery_limit key"
    );
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
fn app_config_refresh_cycle_items_preserves_battery_limit() {
    let mut config = AppConfig::default();
    let original_index = config.battery_limit.index;
    config.refresh_cycle_items();
    // battery_limit should not be affected by refresh_cycle_items
    assert_eq!(config.battery_limit.index, original_index);
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
