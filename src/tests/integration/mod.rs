//! Integration tests for inter-component behaviour.
//!
//! These tests use `MockDeviceController` to exercise pipelines that would
//! otherwise require real hardware.

mod mock_device;

use crate::core::traits::DeviceController;
use crate::razer::config::AppConfig;
use crate::razer::enums::{BATTERY_LIMITS, PERF_MODES, RGB_EFFECTS};
use mock_device::MockDeviceController;

// ── MockDeviceController wiring ──────────────────────────────────────────────

#[test]
fn mock_device_initialize_records_call() {
    let mock = MockDeviceController::new();
    mock.initialize(true);
    assert!(
        mock.initialize_called
            .load(std::sync::atomic::Ordering::SeqCst),
        "initialize() must set the initialize_called flag"
    );
}

#[test]
fn mock_device_sleep_records_call() {
    let mock = MockDeviceController::new();
    mock.sleep();
    assert!(
        mock.sleep_called.load(std::sync::atomic::Ordering::SeqCst),
        "sleep() must set the sleep_called flag"
    );
}

#[test]
fn mock_device_shutdown_returns_true() {
    let mock = MockDeviceController::new();
    assert!(
        mock.shutdown().unwrap_or(false),
        "mock shutdown must return true"
    );
}

#[test]
fn mock_device_persist_records_call() {
    let mock = MockDeviceController::new();
    mock.persist_config();
    assert!(
        mock.persist_called
            .load(std::sync::atomic::Ordering::SeqCst),
        "persist_config() must set the persist_called flag"
    );
}

// ── CycleState integration with AppConfig ───────────────────────────────────

#[test]
fn app_config_battery_limit_cycle_covers_all_items() {
    let mut config = AppConfig::default();
    let total = config.battery_limit.items.len();
    // Cycle through all items and confirm we return to the start
    for _ in 0..total {
        config.battery_limit.next();
    }
    assert_eq!(
        config.battery_limit.index, 0,
        "after a full cycle, battery_limit index must return to 0"
    );
}

#[test]
fn perf_modes_constant_has_six_entries() {
    assert_eq!(PERF_MODES.len(), 6);
}

#[test]
fn rgb_effects_constant_has_six_entries() {
    assert_eq!(RGB_EFFECTS.len(), 6);
}

#[test]
fn battery_limits_constant_has_eight_entries() {
    assert_eq!(BATTERY_LIMITS.len(), 8);
}

#[test]
fn mock_device_cycle_perf_records_call() {
    let mock = MockDeviceController::new();
    mock.cycle_perf_mode();
    assert!(
        mock.cycle_perf_called
            .load(std::sync::atomic::Ordering::SeqCst),
        "cycle_perf_mode() must set cycle_perf_called flag"
    );
}

#[test]
fn mock_device_clone_shares_state() {
    let mock = MockDeviceController::new();
    let clone = mock.clone();
    // Calling initialize on the clone must be visible via the original's Arc
    clone.initialize(false);
    assert!(
        mock.initialize_called
            .load(std::sync::atomic::Ordering::SeqCst),
        "cloned mock must share Arc state with original"
    );
}
