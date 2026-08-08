mod common;

use blade_controlhub::hal::DeviceController;
use blade_controlhub::razer::enums::{BATTERY_LIMITS, PERF_MODES, PerfMode, RGB_EFFECTS};
use common::mock_device::MockDeviceController;

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
    assert!(
        mock.sleep().unwrap_or(false),
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

#[test]
fn perf_modes_constant_uses_ui_order() {
    assert_eq!(
        PERF_MODES,
        [
            PerfMode::BatterySaver,
            PerfMode::Silent,
            PerfMode::Quiet,
            PerfMode::Balanced,
            PerfMode::Performance,
            PerfMode::Turbo,
            PerfMode::Custom,
        ]
    );
}

#[test]
fn rgb_effects_constant_has_six_entries() {
    assert_eq!(RGB_EFFECTS.len(), 7);
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
    clone.initialize(false);
    assert!(
        mock.initialize_called
            .load(std::sync::atomic::Ordering::SeqCst),
        "cloned mock must share Arc state with original"
    );
}
