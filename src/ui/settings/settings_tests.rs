use super::*;
use crate::config::ThemeColor;
use crate::razer::{
    config::{AppConfig, PowerProfile},
    enums::PerfMode,
};
use crate::runtime::settings_state::SettingsState;
use crate::ui::settings::SettingsCommand;
use std::time::Instant;

#[test]
fn settings_icon_renderer_honors_requested_native_size() {
    let icon = Settings::load_settings_icon_with_size(ThemeColor::new(0, 128, 255), 32);
    assert_eq!(icon.width, 32);
    assert_eq!(icon.rgba.len(), 32 * 32 * 4);
}
#[test]
fn settings_icon_renderer_tints_visible_pixels_with_theme_color() {
    let icon = Settings::load_settings_icon_with_size(ThemeColor::new(255, 0, 0), 64);
    assert!(
        icon.rgba
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 0 && pixel[0] > pixel[1] && pixel[0] > pixel[2])
            .count()
            > 0
    );
}
#[test]
fn unsupported_perf_mode_notice_mentions_selected_mode() {
    let mut settings = Settings::new();
    settings.flash_unsupported_perf_mode(PerfMode::Performance);
    assert_eq!(
        settings.unsupported_perf_mode_message(),
        Some("\"Performance\" mode not supported".to_string())
    );
}
#[test]
fn unsupported_perf_mode_notice_expires() {
    let mut settings = Settings::new();
    settings.unsupported_perf_mode_notice = Some(UnsupportedPerfModeNotice {
        mode: PerfMode::Turbo,
        shown_at: Instant::now() - UNSUPPORTED_PERF_MODE_NOTICE_DURATION,
    });
    assert_eq!(settings.unsupported_perf_mode_message(), None);
    assert!(settings.unsupported_perf_mode_notice.is_none());
}
#[test]
fn update_state_reflects_runtime_perf_mode_changes() {
    let mut settings = Settings::new();
    let mut initial = SettingsState::from(AppConfig::default());
    initial.ac_profile.perf_mode = PerfMode::Silent;
    settings.show(initial);
    let mut refreshed = SettingsState::from(AppConfig::default());
    refreshed.ac_profile.perf_mode = PerfMode::Turbo;
    settings.update_state(refreshed);
    assert_eq!(
        settings
            .state
            .as_ref()
            .expect("settings state should be present")
            .ac_profile
            .perf_mode,
        PerfMode::Turbo
    );
}
#[test]
fn update_state_preserves_user_selected_profile_after_runtime_refresh() {
    let mut settings = Settings::new();
    settings.show(SettingsState::from(AppConfig::default()));
    settings.selected_profile = PowerProfile::Battery;
    settings.update_state(SettingsState::from(AppConfig::default()));
    assert_eq!(settings.selected_profile, PowerProfile::Battery);
}
#[test]
fn theme_color_preview_defers_the_ipc_command_until_the_picker_settles() {
    let mut settings = Settings::new();
    settings.show(SettingsState::from(AppConfig::default()));
    let preview = ThemeColor::new(25, 100, 200);
    settings.preview_theme_color(preview);
    assert_eq!(
        settings.state.as_ref().map(|state| state.theme_color),
        Some(preview)
    );
    assert!(settings.drain_commands().is_empty());
    settings
        .pending_theme_color
        .as_mut()
        .expect("preview should be pending")
        .last_changed_at = Instant::now() - super::THEME_COLOR_COMMIT_DEBOUNCE;
    assert!(settings.commit_pending_theme_color_if_due());
    assert_eq!(
        settings.drain_commands(),
        vec![SettingsCommand::SetThemeColor(preview)]
    );
}
#[test]
fn runtime_refresh_does_not_overwrite_a_pending_theme_preview() {
    let mut settings = Settings::new();
    settings.show(SettingsState::from(AppConfig::default()));
    let preview = ThemeColor::new(25, 100, 200);
    settings.preview_theme_color(preview);
    settings.update_state(SettingsState::from(AppConfig::default()));
    assert_eq!(
        settings.state.as_ref().map(|state| state.theme_color),
        Some(preview)
    );
}
