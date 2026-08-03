use eframe::egui::{self, Context};
use std::time::Duration;

use crate::config::ThemeColor;
use crate::razer::{
    config::PowerProfile,
    enums::{BatteryLimit, PerfMode, RGBEffect},
};
use crate::ui::theme::{theme_color32, theme_text_color};

mod device_tab;
mod key_mapping_tab;
mod settings_model;
mod settings_tab;
pub mod store;

pub use settings_model::Settings;

pub const THEME_COLOR_COMMIT_DEBOUNCE: Duration = Duration::from_millis(180);

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
