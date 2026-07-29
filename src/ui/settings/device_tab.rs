/// Device tab UI component.
///
/// Renders AC/Battery profile-specific controls.
use eframe::egui;

use crate::razer::{
    config::PowerProfile,
    enums::{PerfMode, RGBEffect},
};
use crate::runtime::settings_state::DeviceProfileState;
use crate::ui::theme::perf_mode_color32;

use super::Settings;
use super::SettingsCommand;

pub fn show(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            profile_switcher(ui, settings);
            ui.add_space(8.0);
            performance_section(ui, ctx, settings);
            ui.add_space(8.0);
            refresh_rate_section(ui, ctx, settings);
            ui.add_space(8.0);
            lighting_section(ui, ctx, settings);
            ui.add_space(8.0);
            other_section(ui, ctx, settings);
        });
}

fn profile_switcher(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.horizontal(|ui| {
        ui.label("Profile");
        ui.selectable_value(&mut settings.selected_profile, PowerProfile::Ac, "AC Power");
        ui.selectable_value(
            &mut settings.selected_profile,
            PowerProfile::Battery,
            "Battery",
        );

        if settings
            .state
            .as_ref()
            .is_some_and(|state| state.current_profile == settings.selected_profile)
        {
            ui.label("Active");
        }
    });
}

fn performance_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    let profile = settings.selected_profile;
    let profile_state = selected_profile_state(settings);
    let perf_mode = profile_state
        .as_ref()
        .map(|state| state.perf_mode)
        .unwrap_or(PerfMode::Unknown);

    section_with_header(
        ui,
        |ui| performance_header(ui, perf_mode),
        |ui| {
            let Some(profile_state) = profile_state else {
                ui.label("Waiting for runtime state...");
                return;
            };

            ui.horizontal_wrapped(|ui| {
                for mode in profile_state.perf_modes {
                    let selected = mode == profile_state.perf_mode;
                    if ui.selectable_label(selected, mode.to_string()).clicked() && !selected {
                        set_perf_mode(settings, profile, mode);
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    }
                }
            });
        },
    );
}

fn refresh_rate_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    let profile = settings.selected_profile;
    section(ui, "Refresh Rate", |ui| {
        let Some(profile_state) = selected_profile_state(settings) else {
            ui.label("Waiting for runtime state...");
            return;
        };

        if profile_state.supported_refresh_rates.is_empty() {
            ui.label("No refresh rates detected");
            return;
        }

        ui.horizontal_wrapped(|ui| {
            for hz in profile_state.supported_refresh_rates {
                let selected = hz == profile_state.refresh_rate;
                if ui.selectable_label(selected, format!("{hz} Hz")).clicked() && !selected {
                    set_refresh_rate(settings, profile, hz);
                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                }
            }
        });
    });
}

fn lighting_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    let profile = settings.selected_profile;
    section(ui, "Lighting", |ui| {
        let Some(profile_state) = selected_profile_state(settings) else {
            ui.label("Waiting for runtime state...");
            return;
        };

        let mut brightness = profile_state.keyboard_brightness;
        ui.horizontal(|ui| {
            ui.label("Keyboard Brightness");
            ui.add(
                egui::Slider::new(&mut brightness, 0_u8..=255_u8)
                    .show_value(false)
                    .step_by(51.0),
            );
            ui.label((brightness / 51).to_string());
        });
        if brightness != profile_state.keyboard_brightness {
            set_keyboard_brightness(settings, profile, brightness);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }

        let mut selected_effect = profile_state.rgb_effect;
        egui::ComboBox::from_id_source(format!("rgb-effect-{profile:?}"))
            .selected_text(selected_effect.to_string())
            .show_ui(ui, |ui| {
                for effect in &profile_state.rgb_effects {
                    ui.selectable_value(&mut selected_effect, *effect, effect.to_string());
                }
            });

        if selected_effect != profile_state.rgb_effect {
            set_rgb_effect(settings, profile, selected_effect);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}

fn other_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    let profile = settings.selected_profile;
    section(ui, "Other", |ui| {
        let Some(profile_state) = selected_profile_state(settings) else {
            ui.label("Waiting for runtime state...");
            return;
        };

        let mut enabled = profile_state.underglow_enabled;
        if ui.checkbox(&mut enabled, "Vapour Chamber Light").changed() {
            set_under_glow(settings, profile, enabled);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}

fn section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    section_with_header(
        ui,
        |ui| {
            ui.label(title);
        },
        add_contents,
    );
}

fn section_with_header(
    ui: &mut egui::Ui,
    add_header: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(8.0, 7.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_header(ui);
            ui.separator();
            add_contents(ui);
        });
}

fn performance_header(ui: &mut egui::Ui, perf_mode: PerfMode) {
    ui.horizontal(|ui| {
        ui.label("Performance");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new("⏺").color(perf_mode_color32(perf_mode)));
        });
    });
}

fn selected_profile_state(settings: &Settings) -> Option<DeviceProfileState> {
    settings
        .state
        .as_ref()
        .map(|state| state.profile(settings.selected_profile).clone())
}

fn set_perf_mode(settings: &mut Settings, profile: PowerProfile, mode: PerfMode) {
    if let Some(state) = settings.state.as_mut() {
        state.profile_mut(profile).perf_mode = mode;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetPerfMode(profile, mode));
    }
}

fn set_refresh_rate(settings: &mut Settings, profile: PowerProfile, hz: u32) {
    if let Some(state) = settings.state.as_mut() {
        state.profile_mut(profile).refresh_rate = hz;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetRefreshRate(profile, hz));
    }
}

fn set_keyboard_brightness(settings: &mut Settings, profile: PowerProfile, brightness: u8) {
    if let Some(state) = settings.state.as_mut() {
        state.profile_mut(profile).keyboard_brightness = brightness;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetKeyboardBrightness(profile, brightness));
    }
}

fn set_rgb_effect(settings: &mut Settings, profile: PowerProfile, effect: RGBEffect) {
    if let Some(state) = settings.state.as_mut() {
        state.profile_mut(profile).rgb_effect = effect;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetRgbEffect(profile, effect));
    }
}

fn set_under_glow(settings: &mut Settings, profile: PowerProfile, enabled: bool) {
    if let Some(state) = settings.state.as_mut() {
        state.profile_mut(profile).underglow_enabled = enabled;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetUnderGlow(profile, enabled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::razer::config::AppConfig;
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
}
