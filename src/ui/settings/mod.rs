use eframe::egui::{self, Context};
use std::time::Duration;

use crate::config::ThemeColor;
use crate::razer::{
    config::PowerProfile,
    enums::{BatteryLimit, PerfMode, RGBEffect},
};
use crate::ui::theme::{theme_color32, theme_text_color};
use crate::win::system::usbpcap::capture::CapturedCommand;

mod command_lab_tab;
mod device_tab;
mod key_mapping_tab;
mod settings_model;
mod settings_tab;
pub mod store;

pub use settings_model::Settings;

pub const THEME_COLOR_COMMIT_DEBOUNCE: Duration = Duration::from_millis(180);
pub(super) const SECTION_TITLE_SIZE: f32 = 16.0;
pub(super) const CHOICE_BUTTON_HEIGHT: f32 = 27.0;
pub(super) const CHOICE_BUTTONS_PER_ROW: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomModeSetting {
    Cpu,
    Gpu,
}

impl CustomModeSetting {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Gpu => "GPU",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsCommand {
    SetPrimaryMultimediaKeys(bool),
    SetAdvancedExperimentalFeatures(bool),
    SetPerfMode(PowerProfile, PerfMode),
    SetCustomModeConfig { cpu_level: u8, gpu_level: u8 },
    SetFanSpeed(PowerProfile, u8),
    SetRefreshRate(PowerProfile, u32),
    SetKeyboardBrightness(PowerProfile, u8),
    SetRgbEffect(PowerProfile, RGBEffect),
    SetUnderGlow(PowerProfile, bool),
    SetBatteryLimit(BatteryLimit),
    SetThemeColor(ThemeColor),
    BeginRazerKeyCapture { row_idx: usize },
    CancelRazerKeyCapture,
    BeginCommandLabRecord { row_idx: usize },
    CancelCommandLabRecord,
    PlayCommandLabCommands(Vec<CapturedCommand>),
    SaveCommandLabCommands {
        name: String,
        commands: Vec<CapturedCommand>,
    },
    RemoveCommandLabCommand(String),
}

pub fn custom_mode_level_name(level: u8) -> &'static str {
    match level {
        0 => "Low",
        1 => "Medium",
        2 => "High",
        3 => "Max",
        _ => "Unknown",
    }
}

pub(super) fn section_title(title: &str) -> egui::RichText {
    egui::RichText::new(title).size(SECTION_TITLE_SIZE)
}

pub(super) fn choice_button(
    ui: &mut egui::Ui,
    selected: bool,
    label: impl Into<String>,
    width: f32,
) -> egui::Response {
    let label = label.into();
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
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, CHOICE_BUTTON_HEIGHT),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        ui.painter().rect(rect, visuals.rounding, fill, border);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::TextStyle::Button.resolve(ui.style()),
            text_color,
        );
    }

    response
}

fn toggle_switch(ui: &mut egui::Ui, enabled: &mut bool) -> egui::Response {
    let desired_size = egui::vec2(
        ui.spacing().interact_size.x * 0.75,
        ui.spacing().interact_size.y * 0.75,
    );
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if response.clicked() {
        *enabled = !*enabled;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, *enabled);
        let track_rect = rect.expand(visuals.expansion);
        let radius = track_rect.height() / 2.0;
        ui.painter()
            .rect(track_rect, radius, visuals.bg_fill, visuals.bg_stroke);

        let knob_radius = radius - 2.0;
        let knob_x = egui::lerp(
            (track_rect.left() + radius)..=(track_rect.right() - radius),
            ui.ctx().animate_bool(response.id, *enabled),
        );
        ui.painter().circle_filled(
            egui::pos2(knob_x, track_rect.center().y),
            knob_radius,
            visuals.fg_stroke.color,
        );
    }

    response
}

pub(super) fn right_aligned_toggle(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    enabled: &mut bool,
) -> egui::Response {
    let row_size = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(
        row_size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                toggle_switch(ui, enabled)
            })
            .inner
        },
    )
    .inner
}

pub(super) fn primary_tab(ui: &mut egui::Ui, selected_tab: &mut String, label: &str) {
    let selected = selected_tab == label;
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
        *selected_tab = label.to_owned();
    }
}

pub(super) fn loading_screen(ui: &mut egui::Ui, ctx: &Context) {
    ctx.request_repaint_after(Duration::from_millis(16));
    ui.allocate_ui_with_layout(
        ui.available_size(),
        egui::Layout::centered_and_justified(egui::Direction::TopDown),
        |ui| {
            ui.add(egui::Spinner::new().size(50.0));
        },
    );
}

pub(super) fn apply_settings_theme(ctx: &Context, color: ThemeColor) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(7.0, 5.0);
    style.spacing.button_padding = egui::vec2(9.0, 3.0);
    style.spacing.slider_width = 360.0;
    style.spacing.slider_rail_height = 8.0;
    style.visuals.selection.bg_fill = theme_color32(color);
    style.visuals.selection.stroke.color = theme_text_color(color);
    style.visuals.slider_trailing_fill = true;
    style.visuals.hyperlink_color = theme_color32(color);
    ctx.set_style(style);
}
