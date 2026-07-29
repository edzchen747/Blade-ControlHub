use eframe::egui::{self};
use egui::Context;
use resvg::{tiny_skia, usvg};
use tracing::warn;

use crate::runtime::settings_state::SettingsState;
use crate::ui::custom_key_map::CustomKeyMap;
use crate::ui::theme::{SETTINGS_ICON_COLOR, SETTINGS_ICON_SIZE};

mod appearance_tab;
mod device_tab;
mod key_mapping_tab;
pub mod store;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsCommand {
    SetDefaultMultimediaKeys(bool),
    BeginRazerKeyCapture,
    CancelRazerKeyCapture,
}

pub struct Settings {
    pub show: bool,
    pub update: bool,
    pub current_tab: String,
    pub key_map_current_tab: String,
    pub state: Option<SettingsState>,
    pub custom_key_map: CustomKeyMap,
    pending_commands: Vec<SettingsCommand>,
}

impl Settings {
    pub fn new() -> Self {
        Self {
            show: false,
            update: false,
            current_tab: "Device".to_string(),
            key_map_current_tab: "Multimedia Keys".to_string(),
            state: None,
            custom_key_map: CustomKeyMap::new(),
            pending_commands: Vec::new(),
        }
    }

    pub fn show(&mut self, state: SettingsState) {
        self.state = Some(state);
        self.show = true;
        self.update = true;
    }

    pub fn update_state(&mut self, state: SettingsState) {
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
                ui.selectable_value(&mut self.current_tab, "Key Mapping".into(), "Key Mapping");
                ui.selectable_value(&mut self.current_tab, "Appearance".into(), "Appearance");
            });
            ui.separator();

            match self.current_tab.as_str() {
                "Device" => {
                    device_tab::show(ui, ctx, self);
                }
                "Key Mapping" => {
                    key_mapping_tab::show(ui, ctx, self);
                }
                "Appearance" => {
                    appearance_tab::show(ui, ctx);
                }
                _ => {}
            }
        });
    }

    // Make this public so main.rs can access it on startup
    pub fn load_settings_icon() -> egui::IconData {
        let size = SETTINGS_ICON_SIZE;
        let Some(mut pixmap) = tiny_skia::Pixmap::new(size, size) else {
            warn!("Failed to allocate settings icon pixmap; using fallback icon");
            return Self::fallback_settings_icon(size);
        };
        let opt = usvg::Options::default();

        let coloured_svg = include_str!("../../../assets/settings_icon.svg")
            .replace("#FFFFFF", SETTINGS_ICON_COLOR)
            .replace("#ffffff", &SETTINGS_ICON_COLOR.to_lowercase());
        let tree = match usvg::Tree::from_str(&coloured_svg, &opt) {
            Ok(tree) => tree,
            Err(error) => {
                warn!(
                    ?error,
                    "Failed to parse settings icon SVG; using fallback icon"
                );
                return Self::fallback_settings_icon(size);
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

    fn fallback_settings_icon(size: u32) -> egui::IconData {
        let mut rgba = Vec::with_capacity((size * size * 4) as usize);
        for _ in 0..(size * size) {
            rgba.extend_from_slice(&[0xff, 0xd7, 0x00, 0xff]);
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
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}
