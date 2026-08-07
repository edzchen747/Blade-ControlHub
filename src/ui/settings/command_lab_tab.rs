/// Command Lab tab UI component.
///
/// Renders a growing list of command rows. Each row pairs a command text
/// box with a Record/Cancel button that drives the runtime recording
/// countdown through IPC, mirroring the Razer special key capture flow.
use std::time;

use eframe::egui;

use crate::ui::theme::{
    SETTINGS_CONTENT_TOP_SPACING, SETTINGS_KEY_BUTTON_HEIGHT, SETTINGS_KEY_BUTTON_WIDTH,
    SETTINGS_KEY_LISTEN_INTERVAL_MS, SETTINGS_ROW_SPACING, SETTINGS_TEXT_EDIT_WIDTH,
};

static NEW_ROW_ERROR_MESSAGE: &str = "Complete current row to add more";

use super::Settings;
use super::SettingsCommand;
pub fn show(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    ui.label("Record commands and capture what happens");
    ui.add_space(SETTINGS_CONTENT_TOP_SPACING);

    let recording_row_idx = settings.command_lab.recording_row_idx();
    if recording_row_idx.is_some() {
        ctx.request_repaint_after(time::Duration::from_millis(SETTINGS_KEY_LISTEN_INTERVAL_MS));
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut row_to_remove = None;
        let mut record_request = None;
        let mut cancel_request = false;

        for (idx, row) in settings.command_lab.rows.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}:", idx + 1));
                ui.add(
                    egui::TextEdit::singleline(&mut row.command)
                        .hint_text("Command")
                        .desired_width(SETTINGS_TEXT_EDIT_WIDTH),
                );
                ui.label("➡");

                let is_this_row_recording = recording_row_idx == Some(idx);
                let button_label = if is_this_row_recording {
                    "Cancel".to_owned()
                } else {
                    "Record".to_owned()
                };
                let can_toggle_this_row = is_this_row_recording || recording_row_idx.is_none();
                if ui
                    .add_enabled_ui(can_toggle_this_row, |ui| {
                        ui.add_sized(
                            [SETTINGS_KEY_BUTTON_WIDTH, SETTINGS_KEY_BUTTON_HEIGHT],
                            egui::Button::new(button_label),
                        )
                        .clicked()
                    })
                    .inner
                {
                    if is_this_row_recording {
                        cancel_request = true;
                    } else {
                        record_request = Some(idx);
                    }
                }

                let can_delete = recording_row_idx.is_none();
                if ui
                    .add_enabled_ui(can_delete, |ui| ui.button("🗑").clicked())
                    .inner
                {
                    row_to_remove = Some(idx);
                }
            });
            ui.add_space(SETTINGS_ROW_SPACING);
        }

        if let Some(idx) = row_to_remove {
            settings.command_lab.remove_row(idx);
        }
        if let Some(row_idx) = record_request {
            settings.command_lab.set_recording_row_idx(Some(row_idx));
            settings.queue_command(SettingsCommand::BeginCommandLabRecord { row_idx });
        }
        if cancel_request {
            settings.command_lab.set_recording_row_idx(None);
            settings.queue_command(SettingsCommand::CancelCommandLabRecord);
        }

        ui.add_space(SETTINGS_CONTENT_TOP_SPACING);
        ui.separator();

        let can_add = settings.command_lab.can_add_row();
        ui.add_enabled_ui(can_add, |ui| {
            if ui.button("➕ Add New Row").clicked() {
                settings.command_lab.add_row();
            }
        });
        if !can_add {
            ui.weak(NEW_ROW_ERROR_MESSAGE);
        }
    });
}
