use eframe::egui::{self, Context};
use resvg::{tiny_skia, usvg};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;

use crate::config::ThemeColor;
use crate::razer::{config::PowerProfile, enums::PerfMode};
use crate::runtime::settings_state::SettingsState;
use crate::ui::custom_key_map::CustomKeyMap;
use crate::ui::settings::{
    SettingsCommand, THEME_COLOR_COMMIT_DEBOUNCE, apply_settings_theme, loading_screen, primary_tab,
};
use crate::ui::theme::{SETTINGS_ICON_SIZE, SETTINGS_LOADING_ICON_COLOR};

const UNSUPPORTED_PERF_MODE_NOTICE_DURATION: Duration = Duration::from_millis(2000);

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
    unsupported_perf_mode_notice: Option<UnsupportedPerfModeNotice>,
    pending_theme_color: Option<PendingThemeColor>,
}

struct UnsupportedPerfModeNotice {
    mode: PerfMode,
    shown_at: Instant,
}
struct PendingThemeColor {
    color: ThemeColor,
    last_changed_at: Instant,
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
            unsupported_perf_mode_notice: None,
            pending_theme_color: None,
        }
    }
    pub fn show(&mut self, state: SettingsState) {
        self.selected_profile = state.current_profile;
        self.state = Some(state);
        self.show = true;
        self.update = true;
    }
    pub fn update_state(&mut self, mut state: SettingsState) {
        if self
            .state
            .as_ref()
            .is_none_or(|previous| previous.model_name.is_empty())
        {
            self.selected_profile = state.current_profile;
        }
        if let Some(pending) = &self.pending_theme_color {
            state.theme_color = pending.color;
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
    pub fn preview_theme_color(&mut self, color: ThemeColor) {
        if let Some(state) = self.state.as_mut() {
            state.theme_color = color;
            self.pending_theme_color = Some(PendingThemeColor {
                color,
                last_changed_at: Instant::now(),
            });
        }
    }
    pub fn commit_pending_theme_color_if_due(&mut self) -> bool {
        let Some(pending) = self.pending_theme_color.as_ref() else {
            return false;
        };
        if pending.last_changed_at.elapsed() < THEME_COLOR_COMMIT_DEBOUNCE {
            return false;
        }
        let color = pending.color;
        self.pending_theme_color = None;
        self.update = true;
        self.queue_command(SettingsCommand::SetThemeColor(color));
        true
    }
    pub fn cancel_pending_theme_color(&mut self) {
        self.pending_theme_color = None;
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
    pub fn flash_unsupported_perf_mode(&mut self, mode: PerfMode) {
        self.unsupported_perf_mode_notice = Some(UnsupportedPerfModeNotice {
            mode,
            shown_at: Instant::now(),
        });
    }
    pub fn unsupported_perf_mode_message(&mut self) -> Option<String> {
        let notice = self.unsupported_perf_mode_notice.as_ref()?;
        if notice.shown_at.elapsed() >= UNSUPPORTED_PERF_MODE_NOTICE_DURATION {
            self.unsupported_perf_mode_notice = None;
            return None;
        }
        Some(format!("\"{}\" mode not supported", notice.mode))
    }
    pub fn unsupported_perf_mode_notice_duration() -> Duration {
        UNSUPPORTED_PERF_MODE_NOTICE_DURATION
    }
    pub fn ui(&mut self, ctx: &Context) {
        self.apply_current_theme(ctx);
        if self.update {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.update = false;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(2.0);
            if self.state.is_none() {
                loading_screen(ui, ctx);
                return;
            }
            ui.label(
                egui::RichText::new(Self::get_model_name(&self.state))
                    .size(24.0)
                    .color(egui::Color32::from_gray(195)),
            );
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                primary_tab(ui, &mut self.current_tab, "Device");
                primary_tab(ui, &mut self.current_tab, "Settings");
                primary_tab(ui, &mut self.current_tab, "Key Mapping");
            });
            ui.separator();
            ui.add_space(3.0);
            match self.current_tab.as_str() {
                "Device" => super::device_tab::show(ui, ctx, self),
                "Settings" => super::settings_tab::show(ui, ctx, self),
                "Key Mapping" => super::key_mapping_tab::show(ui, ctx, self),
                _ => {}
            }
        });
    }
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
        let colored_svg = include_str!("../../../assets/settings_icon.svg")
            .replace("#FFFFFF", &hex_color)
            .replace("#ffffff", &hex_color.to_lowercase());
        let tree = match usvg::Tree::from_str(&colored_svg, &opt) {
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
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty),
            &mut pixmap.as_mut(),
        );
        egui::IconData {
            rgba: pixmap.take(),
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
        state
            .as_ref()
            .map(|state| state.model_name.clone())
            .unwrap_or_else(|| "Blade Laptop".to_string())
    }
    fn apply_current_theme(&mut self, ctx: &Context) {
        let color = self
            .state
            .as_ref()
            .map(|state| state.theme_color)
            .unwrap_or(SETTINGS_LOADING_ICON_COLOR);
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

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
