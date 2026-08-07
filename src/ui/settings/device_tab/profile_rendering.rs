/// Device tab UI component.
///
/// Renders AC/Battery profile-specific controls.
use eframe::egui;

use crate::razer::{
    config::{CustomModeConfig, PowerProfile, allowed_perf_modes},
    enums::{PerfMode, RGBEffect},
};
use crate::runtime::settings_state::DeviceProfileState;
use crate::ui::theme::perf_mode_color32;

use super::{
    CHOICE_BUTTONS_PER_ROW, CustomModeSetting, Settings, SettingsCommand, choice_button,
    custom_mode_level_name, right_aligned_toggle, section_title,
};

pub fn show(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            profile_switcher(ui, settings);
            ui.add_space(5.0);
            performance_section(ui, ctx, settings);
            ui.add_space(5.0);
            fan_speed_section(ui, ctx, settings);
            ui.add_space(5.0);
            refresh_rate_section(ui, ctx, settings);
            ui.add_space(5.0);
            lighting_section(ui, ctx, settings);
            ui.add_space(5.0);
            other_section(ui, ctx, settings);
        });
}

fn profile_switcher(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.horizontal(|ui| {
        profile_row_label(ui, "Profile");
        profile_tab(ui, settings, PowerProfile::Ac, "AC Power");
        profile_tab(ui, settings, PowerProfile::Battery, "Battery");

        if settings
            .state
            .as_ref()
            .is_some_and(|state| state.current_profile == settings.selected_profile)
        {
            active_profile_indicator(ui);
        }
    });
}

fn profile_row_label(ui: &mut egui::Ui, label: &str) {
    let font = egui::FontId::proportional(16.0);
    let color = ui.visuals().widgets.inactive.fg_stroke.color;
    let width = ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(label.to_owned(), font.clone(), color)
            .size()
            .x
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::hover());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        color,
    );
}

fn active_profile_indicator(ui: &mut egui::Ui) {
    let label = "Active";
    let font = egui::FontId::proportional(14.0);
    let color = ui.visuals().widgets.inactive.fg_stroke.color;
    let width = ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(label.to_owned(), font.clone(), color)
            .size()
            .x
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width + 18.0, 30.0), egui::Sense::hover());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        color,
    );
}

fn profile_tab(ui: &mut egui::Ui, settings: &mut Settings, profile: PowerProfile, label: &str) {
    let selected = settings.selected_profile == profile;
    let font = egui::FontId::proportional(16.0);
    let text_color = if selected {
        ui.visuals().selection.bg_fill
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    let text_width = ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(label.to_owned(), font.clone(), text_color)
            .size()
            .x
    });
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(text_width + 14.0, 30.0), egui::Sense::click());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        text_color,
    );
    if selected {
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + 3.0, rect.bottom() - 2.0),
                egui::pos2(rect.right() - 3.0, rect.bottom() - 2.0),
            ],
            egui::Stroke::new(3.0_f32, ui.visuals().selection.bg_fill),
        );
    }
    if response.clicked() {
        settings.selected_profile = profile;
    }
}

fn performance_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    let profile = settings.selected_profile;
    let profile_state = selected_profile_state(settings);
    let perf_mode = profile_state
        .as_ref()
        .map(|state| state.perf_mode)
        .unwrap_or(PerfMode::Unknown);
    let unsupported_message = settings.unsupported_perf_mode_message();
    let custom_mode_config = settings
        .state
        .as_ref()
        .map(|state| state.custom_mode_config.clone());

    section_with_header(
        ui,
        |ui| performance_header(ui, perf_mode, unsupported_message.as_deref()),
        |ui| {
            let Some(profile_state) = profile_state else {
                ui.label("Waiting for runtime state...");
                return;
            };

            let modes = allowed_perf_modes(profile);
            for row in modes.chunks(CHOICE_BUTTONS_PER_ROW) {
                ui.columns(CHOICE_BUTTONS_PER_ROW, |columns| {
                    for (column, mode) in row.iter().copied().enumerate() {
                        let ui = &mut columns[column];
                        let width = ui.available_width();
                        let selected = mode == profile_state.perf_mode;
                        let available = profile_state.perf_modes.contains(&mode);
                        if available {
                            if choice_button(ui, selected, mode.to_string(), width).clicked()
                                && !selected
                            {
                                handle_perf_mode_click(
                                    settings,
                                    ctx,
                                    profile,
                                    &profile_state,
                                    mode,
                                );
                            }
                        } else if unsupported_perf_mode_button(ui, profile, mode, width).clicked() {
                            handle_perf_mode_click(settings, ctx, profile, &profile_state, mode);
                        }
                    }
                });
            }

            if profile == PowerProfile::Ac
                && profile_state.perf_mode == PerfMode::Custom
                && let Some(custom_mode_config) = custom_mode_config.as_ref()
            {
                custom_mode_config_controls(ui, ctx, settings, custom_mode_config);
            }
        },
    );
}

fn custom_mode_config_controls(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    settings: &mut Settings,
    config: &CustomModeConfig,
) {
    ui.add_space(6.0);
    custom_mode_level_slider(ui, ctx, settings, CustomModeSetting::Cpu, config.cpu_level);
    custom_mode_level_slider(ui, ctx, settings, CustomModeSetting::Gpu, config.gpu_level);
}

fn custom_mode_level_slider(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    settings: &mut Settings,
    setting: CustomModeSetting,
    selected_level: u8,
) {
    let mut level = selected_level;
    let level_label = custom_mode_level_name(level);
    let row_width = ui.available_width();
    let setting_label_width = ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(
                setting.label().to_owned(),
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().widgets.inactive.fg_stroke.color,
            )
            .size()
            .x
    });
    let level_label_width = ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(
                level_label.to_owned(),
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().widgets.inactive.fg_stroke.color,
            )
            .size()
            .x
    });
    let slider_width = (row_width
        - setting_label_width
        - level_label_width
        - 2.0 * ui.spacing().item_spacing.x)
        .max(0.0);
    let row_size = egui::vec2(row_width, ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(
        row_size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(setting.label());
            ui.scope(|ui| {
                ui.spacing_mut().slider_width = slider_width;
                ui.add(
                    egui::Slider::new(&mut level, 0_u8..=3_u8)
                        .show_value(false)
                        .step_by(1.0),
                );
            });
            ui.label(level_label);
        },
    );

    if level != selected_level {
        set_custom_mode_config(settings, setting, level);
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
}

fn fan_speed_section(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    let profile = settings.selected_profile;
    let fan_speed_limits = settings
        .state
        .as_ref()
        .map(|state| state.fan_speed_limits)
        .unwrap_or_default();
    section(ui, "Fan Speed", |ui| {
        let Some(profile_state) = selected_profile_state(settings) else {
            ui.label("Waiting for runtime state...");
            return;
        };

        let fan_speed = profile_state.fan_speeds.get(profile_state.perf_mode);
        ui.columns(CHOICE_BUTTONS_PER_ROW, |columns| {
            let auto_width = columns[0].available_width();
            if choice_button(&mut columns[0], fan_speed == 0, "Auto", auto_width).clicked()
                && fan_speed != 0
            {
                set_fan_speed(settings, profile, 0);
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }

            let fixed_width = columns[1].available_width();
            if choice_button(
                &mut columns[1],
                fan_speed != 0,
                "Fixed Fan Speed",
                fixed_width,
            )
            .clicked()
                && fan_speed == 0
            {
                set_fan_speed(settings, profile, fan_speed_limits.midpoint());
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        });

        if fan_speed != 0 {
            let mut fixed_speed = fan_speed;
            ui.scope(|ui| {
                ui.spacing_mut().slider_width = ui.available_width();
                ui.add(
                    egui::Slider::new(
                        &mut fixed_speed,
                        fan_speed_limits.min..=fan_speed_limits.max,
                    )
                    .show_value(false)
                    .step_by(1.0),
                );
            });
            if fixed_speed != fan_speed {
                set_fan_speed(settings, profile, fixed_speed);
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        }
    });
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

        for row in profile_state
            .supported_refresh_rates
            .chunks(CHOICE_BUTTONS_PER_ROW)
        {
            ui.columns(CHOICE_BUTTONS_PER_ROW, |columns| {
                for (column, hz) in row.iter().copied().enumerate() {
                    let ui = &mut columns[column];
                    let width = ui.available_width();
                    let selected = hz == profile_state.refresh_rate;
                    if choice_button(ui, selected, format!("{hz} Hz"), width).clicked()
                        && !selected
                    {
                        set_refresh_rate(settings, profile, hz);
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    }
                }
            });
        }
    });
}

