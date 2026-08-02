#[cfg(test)]
mod tests {
    use super::*;
    use crate::razer::config::{AppConfig, PowerProfile};
    use crate::runtime::settings_state::SettingsState;

    #[test]
    fn set_perf_mode_updates_selected_profile_only() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_perf_mode(&mut settings, PowerProfile::Battery, PerfMode::Silent);

        let state = settings.state.as_ref().expect("state should be present");
        assert_eq!(state.battery_profile.perf_mode, PerfMode::Silent);
        assert_eq!(state.ac_profile.perf_mode, PerfMode::Balanced);
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetPerfMode(
                PowerProfile::Battery,
                PerfMode::Silent
            )]
        );
    }

    #[test]
    fn set_refresh_rate_updates_selected_profile_only() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from_config(
            AppConfig::default(),
            vec![60, 240],
        ));

        set_refresh_rate(&mut settings, PowerProfile::Battery, 240);

        let state = settings.state.as_ref().expect("state should be present");
        assert_eq!(state.battery_profile.refresh_rate, 240);
        assert_eq!(state.ac_profile.refresh_rate, 0);
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetRefreshRate(PowerProfile::Battery, 240)]
        );
    }

    #[test]
    fn set_fan_speed_updates_selected_profile_and_performance_mode_only() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_fan_speed(&mut settings, PowerProfile::Battery, 20);

        let state = settings.state.as_ref().expect("state should be present");
        assert_eq!(state.battery_profile.fan_speeds.get(PerfMode::Silent), 20);
        assert_eq!(state.ac_profile.fan_speeds.get(PerfMode::Balanced), 0);
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetFanSpeed(PowerProfile::Battery, 20)]
        );
    }

    #[test]
    fn set_custom_mode_config_updates_ac_only_config_and_queues_both_levels() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_custom_mode_config(&mut settings, CustomModeSetting::Gpu, 3);

        let state = settings.state.as_ref().expect("state should be present");
        assert_eq!(state.custom_mode_config.cpu_level, 0);
        assert_eq!(state.custom_mode_config.gpu_level, 3);
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetCustomModeConfig {
                cpu_level: 0,
                gpu_level: 3,
            }]
        );
    }

    #[test]
    fn set_keyboard_brightness_updates_selected_profile_only() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_keyboard_brightness(&mut settings, PowerProfile::Battery, 102);

        let state = settings.state.as_ref().expect("state should be present");
        assert_eq!(state.battery_profile.keyboard_brightness, 102);
        assert_eq!(state.ac_profile.keyboard_brightness, 255);
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetKeyboardBrightness(
                PowerProfile::Battery,
                102
            )]
        );
    }

    #[test]
    fn set_rgb_effect_updates_selected_profile_only() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_rgb_effect(&mut settings, PowerProfile::Battery, RGBEffect::Wave);

        let state = settings.state.as_ref().expect("state should be present");
        assert_eq!(state.battery_profile.rgb_effect, RGBEffect::Wave);
        assert_eq!(state.ac_profile.rgb_effect, RGBEffect::Cycle);
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetRgbEffect(
                PowerProfile::Battery,
                RGBEffect::Wave
            )]
        );
    }

    #[test]
    fn set_under_glow_updates_selected_profile_only() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_under_glow(&mut settings, PowerProfile::Battery, false);

        let state = settings.state.as_ref().expect("state should be present");
        assert!(!state.battery_profile.underglow_enabled);
        assert!(state.ac_profile.underglow_enabled);
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetUnderGlow(PowerProfile::Battery, false)]
        );
    }

    #[test]
    fn perf_mode_click_action_sets_available_unselected_mode() {
        let mut state = SettingsState::from(AppConfig::default());
        state.ac_profile.perf_mode = PerfMode::Silent;

        assert_eq!(
            perf_mode_click_action(&state.ac_profile, PerfMode::Balanced),
            PerfModeClickAction::Set
        );
    }

    #[test]
    fn perf_mode_click_action_ignores_selected_mode() {
        let state = SettingsState::from(AppConfig::default());

        assert_eq!(
            perf_mode_click_action(&state.ac_profile, PerfMode::Balanced),
            PerfModeClickAction::None
        );
    }

    #[test]
    fn perf_mode_click_action_reports_pruned_mode_as_unsupported() {
        let mut state = SettingsState::from(AppConfig::default());
        state
            .ac_profile
            .perf_modes
            .retain(|mode| *mode != PerfMode::Performance);

        assert_eq!(
            perf_mode_click_action(&state.ac_profile, PerfMode::Performance),
            PerfModeClickAction::UnsupportedNotice
        );
    }

    #[test]
    fn unsupported_perf_mode_click_flashes_notice_without_queuing_command() {
        let mut settings = Settings::new();
        let mut state = SettingsState::from(AppConfig::default());
        state
            .ac_profile
            .perf_modes
            .retain(|mode| *mode != PerfMode::Performance);
        settings.show(state.clone());
        let ctx = egui::Context::default();

        handle_perf_mode_click(
            &mut settings,
            &ctx,
            PowerProfile::Ac,
            &state.ac_profile,
            PerfMode::Performance,
        );

        assert!(settings.drain_commands().is_empty());
        assert_eq!(
            settings.unsupported_perf_mode_message(),
            Some("\"Performance\" mode not supported".to_string())
        );
    }
}
