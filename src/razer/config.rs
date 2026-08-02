mod app_config;
mod custom_mode_config;
mod cycle_state;
mod device_state;
mod fan_speed_limits;
mod fan_speeds;
mod power_profile;

pub use app_config::AppConfig;
pub use custom_mode_config::CustomModeConfig;
pub use cycle_state::CycleState;
pub use device_state::DeviceState;
pub use fan_speed_limits::FanSpeedLimits;
pub use fan_speeds::FanSpeeds;
pub use power_profile::PowerProfile;

use crate::razer::enums::PerfMode;
use crate::runtime::debug_mode;

pub fn allowed_perf_modes(profile: PowerProfile) -> Vec<PerfMode> {
    let mut modes = match profile {
        PowerProfile::Ac => vec![
            PerfMode::Silent,
            PerfMode::Quiet,
            PerfMode::Balanced,
            PerfMode::Performance,
            PerfMode::Turbo,
            PerfMode::Custom,
        ],
        PowerProfile::Battery => vec![
            PerfMode::BatterySaver,
            PerfMode::Silent,
            PerfMode::Quiet,
            PerfMode::Balanced,
        ],
    };
    if debug_mode::is_enabled() {
        modes.push(PerfMode::Unsupported);
    }
    modes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeColor;
    use crate::razer::enums::{PERF_MODES, PerfMode};
    use crate::runtime::debug_mode;

    #[test]
    fn set_device_model_updates_pid_and_name() {
        let mut config = AppConfig::default();
        config.set_device_model("0x02c7".to_string(), "Razer Blade Test".to_string());
        let serialized = serde_json::to_value(&config).expect("config must serialize");
        assert_eq!(serialized["model_pid"], "0x02c7");
        assert_eq!(config.model_name, "Razer Blade Test");
    }

    #[test]
    fn app_config_defaults_to_gold_theme_color() {
        assert_eq!(AppConfig::default().theme_color, ThemeColor::default());
    }

    #[test]
    fn profile_mut_updates_only_selected_profile() {
        let mut config = AppConfig::default();
        config.profile_mut(PowerProfile::Battery).key_lvl = 51;
        assert_eq!(config.profile(PowerProfile::Battery).key_lvl, 51);
        assert_eq!(config.profile(PowerProfile::Ac).key_lvl, 255);
    }

    #[test]
    fn default_perf_mode_is_profile_specific() {
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
    fn refresh_cycle_items_uses_profile_specific_perf_modes() {
        let config = AppConfig::default();
        assert_eq!(
            config.profile(PowerProfile::Ac).perf_mode.items,
            allowed_perf_modes(PowerProfile::Ac)
        );
        assert_eq!(
            config.profile(PowerProfile::Battery).perf_mode.items,
            allowed_perf_modes(PowerProfile::Battery)
        );
    }

    #[test]
    fn debug_mode_is_the_only_gate_for_experimental_performance_mode() {
        for profile in [PowerProfile::Ac, PowerProfile::Battery] {
            assert_eq!(
                allowed_perf_modes(profile).contains(&PerfMode::Unsupported),
                debug_mode::is_enabled()
            );
        }
    }

    #[test]
    fn refresh_cycle_items_preserves_allowed_perf_mode_by_value() {
        let mut config = AppConfig::default();
        config
            .profile_mut(PowerProfile::Ac)
            .perf_mode
            .set(&PerfMode::Turbo)
            .expect("turbo is allowed on AC");
        config.refresh_cycle_items();
        assert_eq!(
            config.profile_mut(PowerProfile::Ac).perf_mode.value(),
            PerfMode::Turbo
        );
    }

    #[test]
    fn refresh_cycle_items_falls_back_to_balanced_when_mode_is_disallowed() {
        let mut config = AppConfig::default();
        let state = config.profile_mut(PowerProfile::Ac);
        state.perf_mode.items = PERF_MODES.to_vec();
        state
            .perf_mode
            .set(&PerfMode::BatterySaver)
            .expect("test list contains battery saver");
        config.refresh_cycle_items();
        assert_eq!(
            config.profile_mut(PowerProfile::Ac).perf_mode.value(),
            PerfMode::Balanced
        );
        assert!(
            !config
                .profile(PowerProfile::Ac)
                .perf_mode
                .items
                .contains(&PerfMode::BatterySaver)
        );
    }

    #[test]
    fn fan_speeds_default_to_automatic_for_every_performance_mode() {
        let config = AppConfig::default();
        for perf_mode in PERF_MODES {
            assert_eq!(
                config.profile(PowerProfile::Ac).fan_speeds.get(perf_mode),
                0
            );
            assert_eq!(
                config
                    .profile(PowerProfile::Battery)
                    .fan_speeds
                    .get(perf_mode),
                0
            );
        }
    }

    #[test]
    fn fan_speed_is_saved_per_performance_mode() {
        let mut fan_speeds = FanSpeeds::default();
        fan_speeds.set(PerfMode::Silent, 10);
        fan_speeds.set(PerfMode::Turbo, 46);
        assert_eq!(fan_speeds.get(PerfMode::Silent), 10);
        assert_eq!(fan_speeds.get(PerfMode::Turbo), 46);
        assert_eq!(fan_speeds.get(PerfMode::Balanced), 0);
    }

    #[test]
    fn fan_speeds_clamp_to_firmware_limits_without_changing_auto() {
        let mut fan_speeds = FanSpeeds::default();
        fan_speeds.set(PerfMode::Silent, 10);
        fan_speeds.set(PerfMode::Turbo, 46);
        fan_speeds.clamp_to_limits(FanSpeedLimits { min: 20, max: 30 });
        assert_eq!(fan_speeds.get(PerfMode::Silent), 20);
        assert_eq!(fan_speeds.get(PerfMode::Turbo), 30);
        assert_eq!(fan_speeds.get(PerfMode::Balanced), 0);
    }

    #[test]
    fn fan_speed_limits_midpoint_uses_the_middle_supported_step() {
        assert_eq!(FanSpeedLimits::default().midpoint(), 28);
        assert_eq!(FanSpeedLimits { min: 20, max: 31 }.midpoint(), 25);
    }

    #[test]
    fn custom_mode_config_defaults_to_low_cpu_and_gpu_levels() {
        let config = AppConfig::default();
        assert_eq!(config.custom_mode_config.cpu_level, 0);
        assert_eq!(config.custom_mode_config.gpu_level, 0);
    }

    #[test]
    fn refresh_cycle_items_clamps_custom_mode_levels_to_firmware_range() {
        let mut config = AppConfig::default();
        config.custom_mode_config.cpu_level = u8::MAX;
        config.custom_mode_config.gpu_level = 4;
        config.refresh_cycle_items();
        assert_eq!(config.custom_mode_config.cpu_level, 3);
        assert_eq!(config.custom_mode_config.gpu_level, 3);
    }
}
