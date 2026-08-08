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
use crate::win::system::usbpcap::{
    USBPCAP_DOWNLOAD_URL, UsbpcapStatus, usbpcap_driver_label, usbpcap_status,
};

static NEW_ROW_ERROR_MESSAGE: &str = "Complete current row to add more";

static USBPCAP_CAPTURE_DESCRIPTION: &str = "Record USB commands for your Razer device while you make changes in Synapse to replay commands from Blade ControlHub.\nUSBPcap is a Windows driver that is used to capture the USB reports sent between Synapse and your device. It requires administrator privileges to start a capture.";

/// Red that keeps ~4:1 contrast on both the light and dark settings themes.
static ADVANCED_EXPERIMENTAL_FEATURES_COLOR: egui::Color32 =
    egui::Color32::from_rgb(0xE5, 0x39, 0x35);

use super::Settings;
use super::SettingsCommand;
use super::settings_tab::ADVANCED_EXPERIMENTAL_FEATURES_DESCRIPTION;
pub fn show(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    ui.label(
        egui::RichText::new(ADVANCED_EXPERIMENTAL_FEATURES_DESCRIPTION)
            .color(ADVANCED_EXPERIMENTAL_FEATURES_COLOR),
    );
    ui.add_space(3.0);
    ui.label(USBPCAP_CAPTURE_DESCRIPTION);
    ui.add_space(3.0);
    let usbpcap_status = usbpcap_status();
    ui.horizontal(|ui| {
        ui.label(usbpcap_driver_label(usbpcap_status));
        if usbpcap_status == UsbpcapStatus::NotInstalled {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.hyperlink_to("Download", USBPCAP_DOWNLOAD_URL);
                ui.label("USBPcap Driver ");
            });
        }
    });
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

                if settings.command_lab.captured_row_idx == Some(idx) {
                    ui.label(format!(
                        "{} commands",
                        settings.command_lab.captured_commands
                    ));
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
            settings.command_lab.begin_capture(row_idx);
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
