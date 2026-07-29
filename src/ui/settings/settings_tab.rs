/// Settings tab UI component.
///
/// Renders global application and device settings that are not tied to a power profile.
use eframe::egui;

use crate::config::ThemeColor;
use crate::razer::enums::BatteryLimit;

use super::{Settings, SettingsCommand};

pub fn show(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            battery_limit_section(ui, ctx, settings);
            ui.add_space(8.0);
            default_func_key_section(ui, ctx, settings);
            ui.add_space(8.0);
            theme_section(ui, ctx, settings);
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

        ui.horizontal(|ui| {
            ui.add(
                egui::Slider::new(&mut index, 0..=max_index)
                    .show_value(false)
                    .step_by(1.0),
            );
            ui.label(battery_limit_label(current));
        });

        if let Some(limit) = limits.get(index as usize).copied()
            && limit != current
        {
            set_battery_limit(settings, limit);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}

fn default_func_key_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    section(ui, "Default Function Key Behaviour", |ui| {
        default_func_key_switcher(ui, ctx, settings);
    });
}

pub(super) fn default_func_key_switcher(
    ui: &mut eframe::egui::Ui,
    ctx: &egui::Context,
    settings: &mut Settings,
) {
    let current_default = default_multimedia_keys(settings);
    let mut selected_default = current_default;

    ui.horizontal(|ui| {
        ui.selectable_value(&mut selected_default, false, "Function");
        ui.selectable_value(&mut selected_default, true, "Multimedia");
    });

    if selected_default != current_default {
        set_default_multimedia_keys(settings, selected_default);
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
}

fn theme_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    section(ui, "Theme", |ui| {
        let mut rgb = theme_color(settings).to_rgb_array();

        ui.horizontal(|ui| {
            ui.label("Accent Color");
            if ui.color_edit_button_rgb(&mut rgb).changed() {
                set_theme_color(settings, ThemeColor::from_rgb_array(rgb));
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            };
            if ui.button("Reset").clicked() {
                set_theme_color(
                    settings,
                    ThemeColor::from_rgb_array(ThemeColor::default().to_rgb_array()),
                );
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        });
    });
}

fn section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(8.0, 7.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(title);
            ui.separator();
            add_contents(ui);
        });
}

fn default_multimedia_keys(settings: &Settings) -> bool {
    settings
        .state
        .as_ref()
        .map(|state| state.default_multimedia_keys)
        .unwrap_or_default()
}

fn theme_color(settings: &Settings) -> ThemeColor {
    settings
        .state
        .as_ref()
        .map(|state| state.theme_color)
        .unwrap_or_default()
}

fn set_default_multimedia_keys(settings: &mut Settings, enabled: bool) {
    if let Some(state) = settings.state.as_mut() {
        state.default_multimedia_keys = enabled;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetDefaultMultimediaKeys(enabled));
    }
}

fn set_battery_limit(settings: &mut Settings, limit: BatteryLimit) {
    if let Some(state) = settings.state.as_mut() {
        state.battery_limit = limit;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetBatteryLimit(limit));
    }
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
    fn set_default_multimedia_keys_updates_settings_state() {
        let mut settings = Settings::new();
        settings.show(SettingsState::from(AppConfig::default()));

        set_default_multimedia_keys(&mut settings, true);

        assert!(default_multimedia_keys(&settings));
        assert!(settings.update);
        assert_eq!(
            settings.drain_commands(),
            vec![SettingsCommand::SetDefaultMultimediaKeys(true)]
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
