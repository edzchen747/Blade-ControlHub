//! Tests for `AppConfig` default values and `refresh_cycle_items()`.

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

// ── refresh_cycle_items() ────────────────────────────────────────────────────

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
