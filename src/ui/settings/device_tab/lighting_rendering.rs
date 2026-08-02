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

fn performance_header(ui: &mut egui::Ui, perf_mode: PerfMode, unsupported_message: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label("Performance");
        if let Some(message) = unsupported_message {
            ui.label(egui::RichText::new(message).color(ui.visuals().warn_fg_color));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new("⏺").color(perf_mode_color32(perf_mode)));
        });
    });
}

fn unsupported_perf_mode_button(
    ui: &mut egui::Ui,
    profile: PowerProfile,
    mode: PerfMode,
    width: f32,
) -> egui::Response {
    let disabled_response = ui.add_enabled(
        false,
        egui::Button::new(mode.to_string()).min_size(egui::vec2(width, 27.0)),
    );
    ui.interact(
        disabled_response.rect,
        ui.make_persistent_id(format!("unsupported-perf-mode-{profile:?}-{mode:?}")),
        egui::Sense::click(),
    )
}

fn choice_button(
    ui: &mut egui::Ui,
    selected: bool,
    label: impl Into<String>,
    width: f32,
) -> egui::Response {
    let accent = ui.visuals().selection.bg_fill;
    let text_color = if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    let border = if selected {
        egui::Stroke::new(1.0_f32, accent)
    } else {
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(82))
    };
    let fill = if selected {
        accent
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label.into()).color(text_color))
            .min_size(egui::vec2(width, 27.0))
            .fill(fill)
            .stroke(border),
    )
}

