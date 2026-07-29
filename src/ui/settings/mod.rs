use eframe::egui::{self};
use egui::Context;
use resvg::{tiny_skia, usvg};
use std::sync::Arc;
use tracing::warn;

use crate::config::ThemeColor;
use crate::razer::{
    config::PowerProfile,
    enums::{BatteryLimit, PerfMode, RGBEffect},
};
use crate::runtime::settings_state::SettingsState;
use crate::ui::custom_key_map::CustomKeyMap;
use crate::ui::theme::{SETTINGS_ICON_SIZE, scaled_theme_color32, theme_color32, theme_text_color};

mod device_tab;
mod key_mapping_tab;
mod settings_tab;
pub mod store;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsCommand {
    SetDefaultMultimediaKeys(bool),
    SetPerfMode(PowerProfile, PerfMode),
    SetRefreshRate(PowerProfile, u32),
    SetKeyboardBrightness(PowerProfile, u8),
    SetRgbEffect(PowerProfile, RGBEffect),
    SetUnderGlow(PowerProfile, bool),
    SetBatteryLimit(BatteryLimit),
    SetThemeColor(ThemeColor),
    BeginRazerKeyCapture,
    CancelRazerKeyCapture,
}

pub struct Settings {
    pub show: bool,
    pub update: bool,
    pub current_tab: String,
    pub key_map_current_tab: String,
    pub selected_profile: PowerProfile,
    pub state: Option<SettingsState>,
    pub custom_key_map: CustomKeyMap,
    applied_icon_color: Option<ThemeColor>,
    pending_commands: Vec<SettingsCommand>,
}

impl Settings {
    pub fn new() -> Self {
        Self {
            show: false,
            update: false,
            current_tab: "Device".to_string(),
            key_map_current_tab: "Multimedia Keys".to_string(),
            selected_profile: PowerProfile::Ac,
            state: None,
            custom_key_map: CustomKeyMap::new(),
            applied_icon_color: None,
            pending_commands: Vec::new(),
        }
    }

    pub fn show(&mut self, state: SettingsState) {
        self.selected_profile = state.current_profile;
        self.state = Some(state);
        self.show = true;
        self.update = true;
    }

    pub fn update_state(&mut self, state: SettingsState) {
        if self
            .state
            .as_ref()
            .is_none_or(|previous| previous.model_name.is_empty())
        {
            self.selected_profile = state.current_profile;
        }
        self.state = Some(state);
    }

    pub fn toggle(&mut self, state: SettingsState) {
        self.show = !self.show;
        if self.show {
            self.show(state);
        }
    }

    pub fn queue_command(&mut self, command: SettingsCommand) {
        self.pending_commands.push(command);
    }

    pub fn drain_commands(&mut self) -> Vec<SettingsCommand> {
        std::mem::take(&mut self.pending_commands)
    }

    pub fn apply_captured_razer_key(&mut self, key_code: u8) {
        let Some(idx) = self.custom_key_map.get_listening_idx() else {
            return;
        };

        self.custom_key_map.reset_key_code(key_code);
        if let Some(row) = self.custom_key_map.razer_keys.get_mut(idx) {
            row.key_code = key_code;
            self.custom_key_map.set_listening_idx(None);
        }
    }

    // Note: The redundant pub fn run(...) has been removed entirely!

    pub fn ui(&mut self, ctx: &Context) {
        self.apply_current_theme(ctx);

        // Force focus if the tray requested an update pop-to-front
        if self.update {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.update = false;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(Self::get_model_name(&self.state));
            ui.separator();

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_tab, "Device".into(), "Device");
                ui.selectable_value(&mut self.current_tab, "Settings".into(), "Settings");
                ui.selectable_value(&mut self.current_tab, "Key Mapping".into(), "Key Mapping");
            });
            ui.separator();

            match self.current_tab.as_str() {
                "Device" => {
                    device_tab::show(ui, ctx, self);
                }
                "Settings" => {
                    settings_tab::show(ui, ctx, self);
                }
                "Key Mapping" => {
                    key_mapping_tab::show(ui, ctx, self);
                }
                _ => {}
            }
        });
    }

    // Make this public so main.rs can access it on startup
    pub fn load_settings_icon(color: ThemeColor) -> egui::IconData {
        Self::load_settings_icon_with_size(color, SETTINGS_ICON_SIZE)
    }

    pub fn load_settings_icon_with_size(color: ThemeColor, size: u32) -> egui::IconData {
        let Some(mut pixmap) = tiny_skia::Pixmap::new(size, size) else {
            warn!("Failed to allocate settings icon pixmap; using fallback icon");
            return Self::fallback_settings_icon(size, color);
        };
        let opt = usvg::Options::default();
        let hex_color = color.to_hex_string();

        let coloured_svg = include_str!("../../../assets/settings_icon.svg")
            .replace("#FFFFFF", &hex_color)
            .replace("#ffffff", &hex_color.to_lowercase());
        let tree = match usvg::Tree::from_str(&coloured_svg, &opt) {
            Ok(tree) => tree,
            Err(error) => {
                warn!(
                    ?error,
                    "Failed to parse settings icon SVG; using fallback icon"
                );
                return Self::fallback_settings_icon(size, color);
            }
        };

        let svg_size = tree.size();
        let scale = (size as f32 / svg_size.width()).min(size as f32 / svg_size.height());

        let tx = (size as f32 - (svg_size.width() * scale)) / 2.0;
        let ty = (size as f32 - (svg_size.height() * scale)) / 2.0;
        let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty);

        resvg::render(&tree, transform, &mut pixmap.as_mut());
        let rgba = pixmap.take();

        egui::IconData {
            rgba,
            width: size,
            height: size,
        }
    }

    fn fallback_settings_icon(size: u32, color: ThemeColor) -> egui::IconData {
        let mut rgba = Vec::with_capacity((size * size * 4) as usize);
        for _ in 0..(size * size) {
            rgba.extend_from_slice(&[color.r, color.g, color.b, 0xff]);
        }
        egui::IconData {
            rgba,
            width: size,
            height: size,
        }
    }

    fn get_model_name(state: &Option<SettingsState>) -> String {
        if let Some(state) = state {
            state.model_name.clone()
        } else {
            // Fallback safely if config isn't populated yet
            "Blade Laptop".to_string()
        }
    }

    fn apply_current_theme(&mut self, ctx: &Context) {
        let color = self
            .state
            .as_ref()
            .map(|state| state.theme_color)
            .unwrap_or_default();

        apply_settings_theme(ctx, color);

        if self.applied_icon_color == Some(color) {
            return;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(Arc::new(
            Self::load_settings_icon(color),
        ))));
        self.applied_icon_color = Some(color);
    }
}

fn apply_settings_theme(ctx: &Context, color: ThemeColor) {
    let accent = theme_color32(color);
    let accent_soft = scaled_theme_color32(color, 0.45);
    let accent_hover = scaled_theme_color32(color, 0.65);
    let selected_text = theme_text_color(color);

    let mut style = (*ctx.style()).clone();
    style.visuals.selection.bg_fill = accent;
    style.visuals.selection.stroke.color = selected_text;
    style.visuals.hyperlink_color = accent;
    style.visuals.widgets.hovered.bg_fill = accent_soft;
    style.visuals.widgets.hovered.bg_stroke.color = accent;
    style.visuals.widgets.active.bg_fill = accent_hover;
    style.visuals.widgets.active.bg_stroke.color = accent;
    style.visuals.widgets.open.bg_fill = accent_soft;
    style.visuals.widgets.open.bg_stroke.color = accent;
    ctx.set_style(style);
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_icon_renderer_honors_requested_native_size() {
        let icon = Settings::load_settings_icon_with_size(ThemeColor::new(0, 128, 255), 32);

        assert_eq!(icon.width, 32);
        assert_eq!(icon.height, 32);
        assert_eq!(icon.rgba.len(), 32 * 32 * 4);
    }

    #[test]
    fn settings_icon_renderer_tints_visible_pixels_with_theme_color() {
        let icon = Settings::load_settings_icon_with_size(ThemeColor::new(255, 0, 0), 64);

        let red_pixels = icon
            .rgba
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 0 && pixel[0] > pixel[1] && pixel[0] > pixel[2])
            .count();

        assert!(
            red_pixels > 0,
            "settings icon should contain red themed pixels"
        );
    }
}
