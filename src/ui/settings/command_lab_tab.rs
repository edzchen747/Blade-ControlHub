/// Command Lab tab UI component.
///
/// Renders a growing list of command rows. Each row pairs a command text
/// box with a Record/Cancel button that drives the runtime recording
/// countdown through IPC, mirroring the Razer special key capture flow.
use std::time;

use eframe::egui;

use crate::ui::command_lab::{NEW_ROW_ERROR_MESSAGE, command_lab_code_preview, format_command_full};
use crate::ui::theme::{
    SETTINGS_CONTENT_TOP_SPACING, SETTINGS_KEY_BUTTON_HEIGHT, SETTINGS_KEY_BUTTON_WIDTH,
    SETTINGS_KEY_LISTEN_INTERVAL_MS, SETTINGS_ROW_SPACING, SETTINGS_TEXT_EDIT_WIDTH,
};
use crate::win::system::usbpcap::{
    USBPCAP_DOWNLOAD_URL, UsbpcapStatus, usbpcap_driver_label, usbpcap_status,
};

/// Common troubleshooting questions and answers for the Command Lab.
static COMMAND_LAB_HELP: [(&str, &str); 2] = [
    (
        "What does \"Failed, too many commands\" mean?",
        "This means there are too many commands being captured in the 5 seconds window. Set the keyboard RGB to static or change the keyboard backlight to be set by dynamic lighting instead of chroma.",
    ),
    (
        "Commands are captured but replay is not working?",
        "Some Razer functionalities such as Snap Tap and game mode are not simply USB commands and are run by Synapse itself.",
    ),
];

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
        let mut play_request = None;
        let mut save_request = None;
        let duplicated_names: Vec<bool> = (0..settings.command_lab.rows.len())
            .map(|idx| settings.command_lab.row_name_is_duplicate(idx))
            .collect();
        let ready_to_save: Vec<bool> = (0..settings.command_lab.rows.len())
            .map(|idx| settings.command_lab.row_ready_to_save(idx))
            .collect();

        for (idx, row) in settings.command_lab.rows.iter_mut().enumerate() {
            // Bring the code block back once the too-many notice expired.
            row.expire_too_many_notice(time::Instant::now());
            ui.horizontal(|ui| {
                ui.label(format!("{}:", idx + 1));
                let mut name_edit = egui::TextEdit::singleline(&mut row.command)
                    .hint_text("Command")
                    .desired_width(SETTINGS_TEXT_EDIT_WIDTH);
                if duplicated_names[idx] {
                    name_edit = name_edit.text_color(ADVANCED_EXPERIMENTAL_FEATURES_COLOR);
                }
                let name_response = ui.add(name_edit);
                // Persist the named command list once the name box is left:
                // only for a row that has a valid capture, a non-empty name,
                // and no name clash with another row.
                if name_response.lost_focus() && ready_to_save[idx] {
                    save_request =
                        Some((row.command.trim().to_owned(), row.captured_commands.clone()));
                }

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

                if row.too_many_commands {
                    ui.label(format!(
                        "Failed, too many commands ({})",
                        row.captured_commands.len()
                    ));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can_delete = recording_row_idx.is_none();
                    if ui
                        .add_enabled_ui(can_delete, |ui| ui.button("🗑").clicked())
                        .inner
                    {
                        row_to_remove = Some((idx, row.command.trim().to_owned()));
                    }
                    ui.add_space(4.0);

                    let can_play = !row.captured_commands.is_empty() && !row.too_many_commands;
                    if ui
                        .add_enabled_ui(can_play, |ui| ui.button("▶").clicked())
                        .inner
                    {
                        play_request = Some(row.captured_commands.clone());
                    }
                    ui.add_space(8.0);

                    if !row.captured_commands.is_empty() && !row.too_many_commands {
                        let commands = &row.captured_commands;
                        let preview = command_lab_code_preview(commands);
                        let response = egui::Frame::none()
                            .fill(ui.visuals().code_bg_color)
                            .rounding(egui::Rounding::same(4.0))
                            .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                            .show(ui, |ui| {
                                ui.add_sized(
                                    [
                                        ui.available_width(),
                                        ui.text_style_height(&egui::TextStyle::Monospace),
                                    ],
                                    egui::Label::new(egui::RichText::new(preview).monospace()),
                                )
                            })
                            .response;
                        if response.hovered() {
                            egui::show_tooltip_at_pointer(
                                ctx,
                                egui::Id::new(("command_lab_commands_tooltip", idx)),
                                |ui| {
                                    ui.set_min_width(120.0);
                                    ui.set_max_width(360.0);
                                    for command in commands {
                                        ui.monospace(format_command_full(command));
                                    }
                                },
                            );
                        }
                    }
                });
            });
            ui.add_space(SETTINGS_ROW_SPACING);
        }

        if let Some((idx, name)) = row_to_remove {
            settings.command_lab.remove_row(idx);
            if !name.is_empty() {
                settings.queue_command(SettingsCommand::RemoveCommandLabCommand(name));
            }
        }
        if let Some(row_idx) = record_request {
            settings.command_lab.begin_capture(row_idx);
            settings.queue_command(SettingsCommand::BeginCommandLabRecord { row_idx });
        }
        if cancel_request {
            settings.command_lab.set_recording_row_idx(None);
            settings.queue_command(SettingsCommand::CancelCommandLabRecord);
        }
        if let Some(commands) = play_request {
            settings.queue_command(SettingsCommand::PlayCommandLabCommands(commands));
        }
        if let Some((name, commands)) = save_request {
            settings.queue_command(SettingsCommand::SaveCommandLabCommands { name, commands });
        }

        ui.add_space(SETTINGS_CONTENT_TOP_SPACING);
        ui.separator();

        ui.horizontal(|ui| {
            let can_add = settings.command_lab.can_add_row();
            ui.add_enabled_ui(can_add, |ui| {
                if ui.button("➕ Add New Row").clicked() {
                    settings.command_lab.add_row();
                }
            });
            if !can_add {
                ui.weak(NEW_ROW_ERROR_MESSAGE);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Help").clicked() {
                    settings.command_lab.show_help = !settings.command_lab.show_help;
                }
            });
        });
        if duplicated_names.iter().any(|&duplicate| duplicate) {
            ui.colored_label(ADVANCED_EXPERIMENTAL_FEATURES_COLOR, "Name already used");
        }
        if settings.command_lab.show_help {
            egui::Frame::group(ui.style())
                .fill(ui.visuals().faint_bg_color)
                .rounding(egui::Rounding::same(5.0))
                .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (index, (question, answer)) in COMMAND_LAB_HELP.iter().enumerate() {
                        ui.label(
                            egui::RichText::new(format!("{}. {question}", index + 1)).strong(),
                        );
                        ui.label(*answer);
                        ui.add_space(6.0);
                    }
                });
        }
    });
}
