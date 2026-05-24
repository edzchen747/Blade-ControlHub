/// Key Mapping tab UI component.
///
/// Renders sub-tabs for Function Keys and Razer Special Keys customization.
use std::time;

use eframe::egui;

use crate::ui::custom_key_map::RazerKeyMap;
use crate::ui::theme::{
    SETTINGS_CONTENT_TOP_SPACING, SETTINGS_KEY_BUTTON_HEIGHT, SETTINGS_KEY_BUTTON_WIDTH,
    SETTINGS_KEY_LISTEN_INTERVAL_MS, SETTINGS_ROW_SPACING, SETTINGS_TEXT_EDIT_WIDTH,
};

use super::device_tab::default_func_key_switcher;
use super::settings::Settings;

pub fn show(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut settings.key_map_current_tab,
            "Multimedia Keys".into(),
            "Function Keys",
        );
        ui.selectable_value(
            &mut settings.key_map_current_tab,
            "Razer Special Keys".into(),
            "Razer Special Keys",
        );
    });
    ui.separator();

    match settings.key_map_current_tab.as_str() {
        "Multimedia Keys" => multimedia_key_tab(ui, ctx, settings),
        "Razer Special Keys" => razer_special_key_tab(ui, ctx, settings),
        _ => {}
    }
}

fn multimedia_key_tab(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    ui.label("Remap F1 - F12 Keys when default behaviour is set to multimedia");
    ui.add_space(SETTINGS_CONTENT_TOP_SPACING);
    default_func_key_switcher(ui, ctx, settings);
}

fn razer_special_key_tab(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    ui.label("Remap Speical Razer Keys (e.g. M1, M2, Mic Mute, Trackpad, Performance)");
    ui.add_space(SETTINGS_CONTENT_TOP_SPACING);
    if let Some(idx) = settings.custom_key_map.get_listening_idx() {
        ctx.request_repaint_after(time::Duration::from_millis(SETTINGS_KEY_LISTEN_INTERVAL_MS));
        if let Some(key_code) = settings.custom_key_map.special_key {
            settings.custom_key_map.reset_key_code(key_code);
            if let Some(row) = settings.custom_key_map.razer_keys.get_mut(idx) {
                row.key_code = key_code;
                settings.custom_key_map.set_listening_idx(None);
                settings.custom_key_map.special_key = None;
            }
        }

        // Allow cancelling with Escape
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            settings.custom_key_map.set_listening_idx(None);
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
        if new_listening_idx.is_some() {
            settings.custom_key_map.set_listening_idx(new_listening_idx);
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
            ui.weak("Complete current row to add more");
        }
    });
}
