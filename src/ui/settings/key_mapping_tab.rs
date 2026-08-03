/// Key Mapping tab UI component.
///
/// Renders sub-tabs for Hypershift and Razer Special Keys customization.
use std::time;

use eframe::egui;

use crate::ui::custom_key_map::{HypershiftKeyMap, RazerKeyMap};
use crate::ui::theme::{
    SETTINGS_CONTENT_TOP_SPACING, SETTINGS_KEY_BUTTON_HEIGHT, SETTINGS_KEY_BUTTON_WIDTH,
    SETTINGS_KEY_LISTEN_INTERVAL_MS, SETTINGS_ROW_SPACING, SETTINGS_TEXT_EDIT_WIDTH,
};

static NEW_ROW_ERROR_MESSAGE: &str = "Complete current row to add more";

use super::Settings;
use super::SettingsCommand;
pub fn show(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut settings.key_map_current_tab,
            "Hypershift".into(),
            "Hypershift",
        );
        ui.selectable_value(
            &mut settings.key_map_current_tab,
            "Razer Special Keys".into(),
            "Razer Special Keys",
        );
    });
    ui.separator();

    match settings.key_map_current_tab.as_str() {
        "Hypershift" => hypershift_tab(ui, ctx, settings),
        "Razer Special Keys" => razer_special_key_tab(ui, ctx, settings),
        _ => {}
    }
}

fn hypershift_tab(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    ui.label("Select keyboard keys to attach for Hypershift actions");
    show_duplicate_key_error(ui, ctx, settings);
    ui.add_space(SETTINGS_CONTENT_TOP_SPACING);

    if settings.custom_key_map.hypershift_listening_idx().is_some() {
        ctx.request_repaint_after(time::Duration::from_millis(SETTINGS_KEY_LISTEN_INTERVAL_MS));
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            settings.custom_key_map.set_hypershift_listening_idx(None);
        } else if let Some(key_code) = captured_normal_key_code(ctx)
            && settings.apply_captured_hypershift_key(key_code)
        {
            ctx.request_repaint_after(Settings::duplicate_key_notice_duration());
        }
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut row_to_remove = None;
        let mut new_listening_idx = None;
        let current_listening_idx = settings.custom_key_map.hypershift_listening_idx();

        for (idx, row) in settings
            .custom_key_map
            .hypershift_keys
            .iter_mut()
            .enumerate()
        {
            ui.horizontal(|ui| {
                ui.label(format!("{}:", idx + 1));
                let is_this_row_listening = current_listening_idx == Some(idx);
                let button_label = if is_this_row_listening {
                    "Press key...".to_owned()
                } else {
                    row.key_code
                        .map(normal_key_label)
                        .unwrap_or_else(|| "None".to_owned())
                };

                if ui
                    .add_sized(
                        [SETTINGS_KEY_BUTTON_WIDTH, SETTINGS_KEY_BUTTON_HEIGHT],
                        egui::Button::new(button_label),
                    )
                    .clicked()
                {
                    new_listening_idx = Some(idx);
                }
                ui.label("➡");
                ui.add_enabled_ui(false, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut row.action)
                            .hint_text("Action (coming soon)")
                            .desired_width(SETTINGS_TEXT_EDIT_WIDTH),
                    );
                });
                if ui.button("🗑").clicked() {
                    row_to_remove = Some(idx);
                }
            });
            ui.add_space(SETTINGS_ROW_SPACING);
        }

        if let Some(idx) = row_to_remove {
            settings.custom_key_map.hypershift_keys.remove(idx);
            match settings.custom_key_map.hypershift_listening_idx() {
                Some(listening_idx) if listening_idx == idx => {
                    settings.custom_key_map.set_hypershift_listening_idx(None);
                }
                Some(listening_idx) if listening_idx > idx => {
                    settings
                        .custom_key_map
                        .set_hypershift_listening_idx(Some(listening_idx - 1));
                }
                _ => {}
            }
        }
        if let Some(row_idx) = new_listening_idx {
            settings
                .custom_key_map
                .set_hypershift_listening_idx(Some(row_idx));
        }

        ui.add_space(SETTINGS_CONTENT_TOP_SPACING);
        ui.separator();

        let can_add = settings.custom_key_map.hypershift_listening_idx().is_none()
            && settings
                .custom_key_map
                .hypershift_keys
                .iter()
                .all(|row| row.key_code.is_some());
        ui.add_enabled_ui(can_add, |ui| {
            if ui.button("➕ Add New Row").clicked() {
                settings
                    .custom_key_map
                    .hypershift_keys
                    .push(HypershiftKeyMap::default());
            }
        });
        if !can_add {
            ui.weak(NEW_ROW_ERROR_MESSAGE);
        }
    });
}

fn captured_normal_key_code(ctx: &egui::Context) -> Option<u8> {
    ctx.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                ..
            } => normal_key_code(*key),
            _ => None,
        })
    })
}

/// Converts supported Hypershift input into Windows virtual-key codes.
fn normal_key_code(key: egui::Key) -> Option<u8> {
    use egui::Key;

    match key {
        Key::Num0 => Some(b'0'),
        Key::Num1 => Some(b'1'),
        Key::Num2 => Some(b'2'),
        Key::Num3 => Some(b'3'),
        Key::Num4 => Some(b'4'),
        Key::Num5 => Some(b'5'),
        Key::Num6 => Some(b'6'),
        Key::Num7 => Some(b'7'),
        Key::Num8 => Some(b'8'),
        Key::Num9 => Some(b'9'),
        Key::A => Some(b'A'),
        Key::B => Some(b'B'),
        Key::C => Some(b'C'),
        Key::D => Some(b'D'),
        Key::E => Some(b'E'),
        Key::F => Some(b'F'),
        Key::G => Some(b'G'),
        Key::H => Some(b'H'),
        Key::I => Some(b'I'),
        Key::J => Some(b'J'),
        Key::K => Some(b'K'),
        Key::L => Some(b'L'),
        Key::M => Some(b'M'),
        Key::N => Some(b'N'),
        Key::O => Some(b'O'),
        Key::P => Some(b'P'),
        Key::Q => Some(b'Q'),
        Key::R => Some(b'R'),
        Key::S => Some(b'S'),
        Key::T => Some(b'T'),
        Key::U => Some(b'U'),
        Key::V => Some(b'V'),
        Key::W => Some(b'W'),
        Key::X => Some(b'X'),
        Key::Y => Some(b'Y'),
        Key::Z => Some(b'Z'),
        _ => None,
    }
}

fn normal_key_label(key_code: u8) -> String {
    match key_code {
        b'0'..=b'9' | b'A'..=b'Z' => (key_code as char).to_string(),
        _ => "Unknown".to_owned(),
    }
}

fn razer_special_key_tab(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    ui.label("Remap Speical Razer Keys (e.g. M1, M2, Mic Mute, Trackpad, Performance)");
    show_duplicate_key_error(ui, ctx, settings);
    ui.add_space(SETTINGS_CONTENT_TOP_SPACING);
    if settings.custom_key_map.get_listening_idx().is_some() {
        ctx.request_repaint_after(time::Duration::from_millis(SETTINGS_KEY_LISTEN_INTERVAL_MS));
        if let Some(key_code) = settings.custom_key_map.special_key.take()
            && settings.apply_captured_razer_key(key_code)
        {
            ctx.request_repaint_after(Settings::duplicate_key_notice_duration());
        }

        // Allow cancelling with Escape
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            settings.custom_key_map.set_listening_idx(None);
            settings.queue_command(SettingsCommand::CancelRazerKeyCapture);
        }
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut row_to_remove = None;
        let mut new_listening_idx: Option<usize> = None;
        let current_listening_idx = settings.custom_key_map.get_listening_idx();

        for (idx, row) in settings.custom_key_map.razer_keys.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}:", idx + 1));
                ui.add(
                    egui::TextEdit::singleline(&mut row.name)
                        .hint_text("Key label")
                        .desired_width(SETTINGS_TEXT_EDIT_WIDTH),
                );
                let is_this_row_listening = current_listening_idx == Some(idx);
                let btn_label = if is_this_row_listening {
                    "Press key...".into()
                } else {
                    if row.key_code == 0 {
                        "None".to_string()
                    } else {
                        format!("{:#04X}", row.key_code)
                    }
                };

                if ui
                    .add_sized(
                        [SETTINGS_KEY_BUTTON_WIDTH, SETTINGS_KEY_BUTTON_HEIGHT],
                        egui::Button::new(btn_label),
                    )
                    .clicked()
                {
                    new_listening_idx = Some(idx);
                }
                ui.label("➡");
                ui.add(
                    egui::TextEdit::singleline(&mut row.action)
                        .hint_text("Action")
                        .desired_width(SETTINGS_TEXT_EDIT_WIDTH),
                );
                if ui.button("🗑").clicked() {
                    row_to_remove = Some(idx);
                }
            });
            ui.add_space(SETTINGS_ROW_SPACING);
        }

        if let Some(idx) = row_to_remove {
            settings.custom_key_map.razer_keys.remove(idx);
        }
        if let Some(row_idx) = new_listening_idx {
            settings.queue_command(SettingsCommand::BeginRazerKeyCapture { row_idx });
        }

        ui.add_space(SETTINGS_CONTENT_TOP_SPACING);
        ui.separator();

        let can_add = settings.custom_key_map.get_listening_idx().is_none()
            && settings
                .custom_key_map
                .razer_keys
                .iter()
                .all(|last| !last.name.is_empty() && last.key_code != 0);

        ui.add_enabled_ui(can_add, |ui| {
            if ui.button("➕ Add New Row").clicked() {
                settings
                    .custom_key_map
                    .razer_keys
                    .push(RazerKeyMap::default());
            }
        });

        if !can_add {
            ui.weak(NEW_ROW_ERROR_MESSAGE);
        }
    });
}

fn show_duplicate_key_error(ui: &mut egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    if let Some(message) = settings.duplicate_key_error_message() {
        ui.colored_label(ui.visuals().error_fg_color, message);
        ctx.request_repaint_after(Settings::duplicate_key_notice_duration());
    }
}

#[cfg(test)]
mod tests {
    use super::{normal_key_code, normal_key_label};

    #[test]
    fn normal_key_capture_uses_windows_virtual_key_codes() {
        assert_eq!(normal_key_code(eframe::egui::Key::A), Some(0x41));
        assert_eq!(normal_key_code(eframe::egui::Key::Num5), Some(b'5'));
    }

    #[test]
    fn normal_key_capture_rejects_non_keyboard_commands() {
        assert_eq!(normal_key_code(eframe::egui::Key::Copy), None);
        assert_eq!(normal_key_code(eframe::egui::Key::F1), None);
        assert_eq!(normal_key_code(eframe::egui::Key::F12), None);
        assert_eq!(normal_key_code(eframe::egui::Key::ArrowLeft), None);
        assert_eq!(normal_key_code(eframe::egui::Key::Space), None);
        assert_eq!(normal_key_code(eframe::egui::Key::F25), None);
    }

    #[test]
    fn normal_key_labels_are_human_readable() {
        assert_eq!(normal_key_label(b'A'), "A");
        assert_eq!(normal_key_label(b'5'), "5");
        assert_eq!(normal_key_label(0x25), "Unknown");
    }
}
