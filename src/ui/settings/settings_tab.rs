use eframe::egui;

use crate::config::ThemeColor;
use crate::razer::enums::BatteryLimit;

use super::{
    CHOICE_BUTTONS_PER_ROW, Settings, SettingsCommand, THEME_COLOR_COMMIT_DEBOUNCE, choice_button,
    right_aligned_toggle, section_title,
};

pub(super) const ADVANCED_EXPERIMENTAL_FEATURES_DESCRIPTION: &str = "⚠ Advanced experimental features may not work perfectly on every device, and some configurations may cause unexpected behaviour. You can turn these features off at any time if they do not work as expected in the Settings tab.";

pub(super) const START_WITH_ADMIN_DESCRIPTION: &str = "This starts the app with administrator privileges so you do not need to repeatedly accept UAC prompts for certain actions.";

pub fn show(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            battery_limit_section(ui, ctx, settings);
            ui.add_space(5.0);
            primary_func_key_section(ui, ctx, settings);
            ui.add_space(5.0);
            theme_section(ui, ctx, settings);
            ui.add_space(5.0);
            start_with_admin_section(ui, ctx, settings);
            ui.add_space(5.0);
            start_with_windows_section(ui, ctx, settings);
            ui.add_space(5.0);
            advanced_experimental_features_section(ui, ctx, settings);
        });
}

fn battery_limit_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    section(ui, "Battery Charge Limit", |ui| {
        let Some(state) = settings.state.as_ref() else {
            ui.label("Waiting for runtime state...");
            return;
        };

        let limits = state.battery_limits.clone();
        if limits.is_empty() {
            ui.label("No battery limits available");
            return;
        }

        let current = state.battery_limit;
        let mut index = limits
            .iter()
            .position(|limit| *limit == current)
            .unwrap_or(0) as u32;
        let max_index = limits.len().saturating_sub(1) as u32;

        let slider_changed = ui
            .horizontal(|ui| {
                let response = ui.add(
                    egui::Slider::new(&mut index, 0..=max_index)
                        .show_value(false)
                        .step_by(1.0),
                );
                ui.label(battery_limit_label(current));
                response.changed()
            })
            .inner;

        if let Some(limit) = selected_battery_limit(&limits, current, index, slider_changed) {
            set_battery_limit(settings, limit);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}

fn primary_func_key_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    section(ui, "Primary Function Key Behaviour", |ui| {
        primary_func_key_switcher(ui, ctx, settings);
    });
}

pub(super) fn primary_func_key_switcher(
    ui: &mut eframe::egui::Ui,
    ctx: &egui::Context,
    settings: &mut Settings,
) {
    let current_primary = primary_multimedia_keys(settings);
    let mut selected_primary = current_primary;

    ui.columns(CHOICE_BUTTONS_PER_ROW, |columns| {
        let function_width = columns[0].available_width();
        if choice_button(
            &mut columns[0],
            !selected_primary,
            "Function",
            function_width,
        )
        .clicked()
        {
            selected_primary = false;
        }
        let multimedia_width = columns[1].available_width();
        if choice_button(
            &mut columns[1],
            selected_primary,
            "Multimedia",
            multimedia_width,
        )
        .clicked()
        {
            selected_primary = true;
        }
    });

    if selected_primary != current_primary {
        set_primary_multimedia_keys(settings, selected_primary);
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
}

fn start_with_admin_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    section(ui, "Start with administrator privileges", |ui| {
        ui.label(START_WITH_ADMIN_DESCRIPTION);
        ui.add_space(3.0);

        let Some(state) = settings.state.as_ref() else {
            ui.label("Waiting for runtime state...");
            return;
        };
        let mut enabled = state.start_with_admin;
        if right_aligned_toggle(ui, "Start with administrator privileges", &mut enabled).changed()
        {
            set_start_with_admin(settings, enabled);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}

fn start_with_windows_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    section(ui, "Start with Windows", |ui| {
        let Some(state) = settings.state.as_ref() else {
            ui.label("Waiting for runtime state...");
            return;
        };
        let mut enabled = state.start_with_windows;
        if right_aligned_toggle(ui, "Start with Windows", &mut enabled).changed() {
            set_start_with_windows(settings, enabled);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}

fn theme_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    section(ui, "Theme", |ui| {
        let mut rgb = theme_color(settings).to_rgb_array();
        let mut reset_requested = false;

        ui.horizontal(|ui| {
            ui.label("Accent Color");
            if ui.color_edit_button_rgb(&mut rgb).changed() {
                settings.preview_theme_color(ThemeColor::from_rgb_array(rgb));
                ctx.request_repaint_after(THEME_COLOR_COMMIT_DEBOUNCE);
            }
            if ui.button("Reset").clicked() {
                reset_requested = true;
            }
        });

        if reset_requested {
            settings.cancel_pending_theme_color();
            set_theme_color(
                settings,
                ThemeColor::from_rgb_array(ThemeColor::default().to_rgb_array()),
            );
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        } else if settings.commit_pending_theme_color_if_due() {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}

fn advanced_experimental_features_section(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    settings: &mut Settings,
) {
    section(ui, "Advanced Experimental Features", |ui| {
        ui.label(ADVANCED_EXPERIMENTAL_FEATURES_DESCRIPTION);
        ui.add_space(3.0);

        let Some(state) = settings.state.as_ref() else {
            ui.label("Waiting for runtime state...");
            return;
        };
        let mut enabled = state.advanced_experimental_features;
        if right_aligned_toggle(ui, "Enable Advanced Experimental Features", &mut enabled).changed()
        {
            set_advanced_experimental_features(settings, enabled);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}

fn section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().window_stroke)
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(10.0, 7.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(section_title(title));
            ui.add_space(3.0);
            add_contents(ui);
        });
}

fn primary_multimedia_keys(settings: &Settings) -> bool {
    settings
        .state
        .as_ref()
        .map(|state| state.primary_multimedia_keys)
        .unwrap_or_default()
}

fn theme_color(settings: &Settings) -> ThemeColor {
    settings
        .state
        .as_ref()
        .map(|state| state.theme_color)
        .unwrap_or_default()
}

fn set_primary_multimedia_keys(settings: &mut Settings, enabled: bool) {
    if let Some(state) = settings.state.as_mut() {
        state.primary_multimedia_keys = enabled;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetPrimaryMultimediaKeys(enabled));
    }
}

fn set_battery_limit(settings: &mut Settings, limit: BatteryLimit) {
    if let Some(state) = settings.state.as_mut() {
        state.battery_limit = limit;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetBatteryLimit(limit));
    }
}

fn set_advanced_experimental_features(settings: &mut Settings, enabled: bool) {
    if let Some(state) = settings.state.as_mut() {
        state.advanced_experimental_features = enabled;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetAdvancedExperimentalFeatures(enabled));
    }
}

fn set_start_with_admin(settings: &mut Settings, enabled: bool) {
    if let Some(state) = settings.state.as_mut() {
        state.start_with_admin = enabled;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetStartWithAdmin(enabled));
    }
}

fn set_start_with_windows(settings: &mut Settings, enabled: bool) {
    if let Some(state) = settings.state.as_mut() {
        state.start_with_windows = enabled;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetStartWithWindows(enabled));
    }
}

fn selected_battery_limit(
    limits: &[BatteryLimit],
    current: BatteryLimit,
    index: u32,
    slider_changed: bool,
) -> Option<BatteryLimit> {
    slider_changed
        .then(|| limits.get(index as usize).copied())
        .flatten()
        .filter(|&limit| limit != current)
}

fn set_theme_color(settings: &mut Settings, color: ThemeColor) {
    if let Some(state) = settings.state.as_mut() {
        state.theme_color = color;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetThemeColor(color));
    }
}

fn battery_limit_label(limit: BatteryLimit) -> String {
    limit.to_string().replace("Limit: ", "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::razer::config::AppConfig;
    use crate::runtime::settings_state::SettingsState;

    #[test]
    fn set_primary_multimedia_keys_updates_settings_state() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_primary_multimedia_keys(&mut settings, true);

        assert!(primary_multimedia_keys(&settings));
        assert!(settings.update);
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetPrimaryMultimediaKeys(true)]
        );
    }

    #[test]
    fn set_battery_limit_updates_global_state() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_battery_limit(&mut settings, BatteryLimit::Limit80);

        assert_eq!(
            settings.state.as_ref().map(|state| state.battery_limit),
            Some(BatteryLimit::Limit80)
        );
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetBatteryLimit(BatteryLimit::Limit80)]
        );
    }

    #[test]
    fn set_advanced_experimental_features_updates_settings_state() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_advanced_experimental_features(&mut settings, false);

        assert!(
            !settings
                .state
                .as_ref()
                .expect("settings state is available")
                .advanced_experimental_features
        );
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetAdvancedExperimentalFeatures(false)]
        );
    }

    #[test]
    fn set_start_with_admin_updates_settings_state() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_start_with_admin(&mut settings, true);

        assert!(
            settings
                .state
                .as_ref()
                .expect("settings state is available")
                .start_with_admin
        );
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetStartWithAdmin(true)]
        );
    }

    #[test]
    fn set_start_with_windows_updates_settings_state() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_start_with_windows(&mut settings, true);

        assert!(
            settings
                .state
                .as_ref()
                .expect("settings state is available")
                .start_with_windows
        );
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetStartWithWindows(true)]
        );
    }

    #[test]
    fn battery_limit_does_not_queue_without_slider_interaction() {
        assert_eq!(
            selected_battery_limit(
                &[BatteryLimit::Off, BatteryLimit::Limit80],
                BatteryLimit::Unknown,
                0,
                false,
            ),
            None
        );
    }

    #[test]
    fn battery_limit_queues_the_changed_slider_value() {
        assert_eq!(
            selected_battery_limit(
                &[BatteryLimit::Off, BatteryLimit::Limit80],
                BatteryLimit::Off,
                1,
                true,
            ),
            Some(BatteryLimit::Limit80)
        );
    }

    #[test]
    fn set_theme_color_updates_global_state() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        let color = ThemeColor::new(0, 128, 255);
        set_theme_color(&mut settings, color);

        assert_eq!(
            settings.state.as_ref().map(|state| state.theme_color),
            Some(color)
        );
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetThemeColor(color)]
        );
    }
}
