/// Device tab UI component.
///
/// Renders the device settings panel, including the default function key behavior switcher.
use eframe::egui;

use super::Settings;
use super::SettingsCommand;

pub fn show(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    default_func_key_switcher(ui, ctx, settings);
}

pub(super) fn default_func_key_switcher(
    ui: &mut eframe::egui::Ui,
    ctx: &egui::Context,
    settings: &mut Settings,
) {
    let mut changed = false;
    ui.label("Default Function key behaviour:");
    ui.horizontal(|ui| {
        let current_default = default_multimedia_keys(settings);
        let mut selected_default = current_default;

        changed |= ui
            .selectable_value(&mut selected_default, false, "Function")
            .changed();
        changed |= ui
            .selectable_value(&mut selected_default, true, "Multimedia")
            .changed();

        if changed && selected_default != current_default {
            set_default_multimedia_keys(settings, selected_default);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}

fn default_multimedia_keys(settings: &Settings) -> bool {
    settings
        .state
        .as_ref()
        .map(|state| state.default_multimedia_keys)
        .unwrap_or_default()
}

fn set_default_multimedia_keys(settings: &mut Settings, enabled: bool) {
    if let Some(state) = settings.state.as_mut() {
        state.default_multimedia_keys = enabled;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetDefaultMultimediaKeys(enabled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::razer::config::AppConfig;
    use crate::runtime::settings_state::SettingsState;

    #[test]
    fn set_default_multimedia_keys_updates_settings_config() {
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
    fn default_multimedia_keys_falls_back_to_function_mode_without_config() {
        let settings = Settings::new();

        assert!(!default_multimedia_keys(&settings));
    }
}
