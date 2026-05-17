//! Tests for `From<u8>` implementations on Razer enums.

use crate::razer::enums::{BatteryLimit, PerfMode, RGBEffect};

// ── PerfMode ─────────────────────────────────────────────────────────────────

#[test]
fn perf_mode_from_zero_is_balanced() {
    assert_eq!(PerfMode::from(0), PerfMode::Balanced);
}

#[test]
fn perf_mode_from_one_is_turbo() {
    assert_eq!(PerfMode::from(1), PerfMode::Turbo);
}

#[test]
fn perf_mode_from_two_is_performance() {
    assert_eq!(PerfMode::from(2), PerfMode::Performance);
}

#[test]
fn perf_mode_from_four_is_custom() {
    assert_eq!(PerfMode::from(4), PerfMode::Custom);
}

#[test]
fn perf_mode_from_five_is_silent() {
    assert_eq!(PerfMode::from(5), PerfMode::Silent);
}

#[test]
fn perf_mode_from_six_is_quiet() {
    assert_eq!(PerfMode::from(6), PerfMode::Quiet);
}

#[test]
fn perf_mode_from_invalid_is_unknown() {
    assert_eq!(PerfMode::from(99), PerfMode::Unknown);
}

// ── RGBEffect ────────────────────────────────────────────────────────────────

#[test]
fn rgb_effect_from_four_is_cycle() {
    assert_eq!(RGBEffect::from(4), RGBEffect::Cycle);
}

#[test]
fn rgb_effect_from_one_is_wave() {
    assert_eq!(RGBEffect::from(1), RGBEffect::Wave);
}

#[test]
fn rgb_effect_from_three_is_breathe() {
    assert_eq!(RGBEffect::from(3), RGBEffect::Breathe);
}

#[test]
fn rgb_effect_from_five_is_ambient() {
    assert_eq!(RGBEffect::from(5), RGBEffect::Ambient);
}

#[test]
fn rgb_effect_from_twenty_five_is_starlight() {
    assert_eq!(RGBEffect::from(25), RGBEffect::Starlight);
}

#[test]
fn rgb_effect_from_seventeen_nine_is_reactive() {
    assert_eq!(RGBEffect::from(19), RGBEffect::Reactive);
}

#[test]
fn rgb_effect_from_seven_is_starlight() {
    // Current behavior: 7 maps to Starlight (same as 25).
    // Note: A bug fix would change this to Unknown.
    assert_eq!(RGBEffect::from(7), RGBEffect::Starlight);
}

#[test]
fn rgb_effect_from_invalid_is_unknown() {
    assert_eq!(RGBEffect::from(99), RGBEffect::Unknown);
}

// ── BatteryLimit ─────────────────────────────────────────────────────────────

#[test]
fn battery_limit_from_sixty_is_off() {
    assert_eq!(BatteryLimit::from(60), BatteryLimit::Off);
}

#[test]
fn battery_limit_from_one_seventy_eight_is_limit50() {
    assert_eq!(BatteryLimit::from(178), BatteryLimit::Limit50);
}

#[test]
fn battery_limit_from_two_zero_eight_is_limit80() {
    assert_eq!(BatteryLimit::from(208), BatteryLimit::Limit80);
}

#[test]
fn battery_limit_from_invalid_is_unknown() {
    assert_eq!(BatteryLimit::from(99), BatteryLimit::Unknown);
}
