/// Settings window orchestrator.
///
/// This module manages the Settings viewport lifecycle and delegates rendering
/// to individual tab components (`device_tab`, `key_mapping_tab`, `appearance_tab`).
use std::sync::{Arc, Mutex};

use eframe::egui::{self};
use egui::Context;
use resvg::{tiny_skia, usvg};

use crate::razer::config::AppConfig;
use crate::ui::app::app;
use crate::ui::custom_key_map::CustomKeyMap;
use crate::ui::theme::{
    SETTINGS_ICON_COLOR, SETTINGS_ICON_SIZE, SETTINGS_PADDING_RATIO, SETTINGS_WINDOW_SIZE,
    SETTINGS_WINDOW_TITLE,
};

use super::appearance_tab;
use super::device_tab;
use super::key_mapping_tab;

pub struct Settings {
    pub show: bool,
    pub update: bool,
    pub current_tab: String,
    pub key_map_current_tab: String,
    pub app_config: Option<AppConfig>,
    pub custom_key_map: CustomKeyMap,
}

impl Settings {
    pub fn new() -> Self {
        Self {
            show: false,
            update: false,
            current_tab: "Device".to_string(),
            key_map_current_tab: "Multimedia Keys".to_string(),
            app_config: None,
            custom_key_map: CustomKeyMap::new(),
        }
    }

    pub fn show(&mut self, config: AppConfig) {
        self.app_config = Some(config);
        self.show = true;
        self.update = true;
    }

    pub fn toggle(&mut self, config: AppConfig) {
        self.show = !self.show;
        if self.show {
            self.show(config);
        }
    }

    pub fn run(ctx: &egui::Context, config_window: Arc<Mutex<Self>>) {
        let config_handle = config_window.clone();
        if !config_handle.lock().unwrap().show {
            return;
        }

        let icon_data = Arc::new(load_settings_icon());

        let config_handle = config_window.clone();

        let (screen_width, screen_height) = ctx.input(|i| {
            // Access the specific monitor this window is currently on
            if let Some(monitor) = i.viewport().monitor_size {
                (monitor.x, monitor.y)
            } else {
                (1920.0, 1080.0)
            }
        });

        let window_size = SETTINGS_WINDOW_SIZE;
        let padding = screen_height * SETTINGS_PADDING_RATIO;
        let spawn_pos = egui::pos2(
            screen_width - window_size.x - padding * 0.1,
            screen_height - window_size.y - padding,
        );

        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("settings_window"),
            egui::ViewportBuilder::default()
                .with_title(SETTINGS_WINDOW_TITLE)
                .with_icon(icon_data.clone())
                .with_mouse_passthrough(false)
                .with_position(spawn_pos)
                .with_inner_size(window_size)
                .with_min_inner_size(window_size)
                .with_max_inner_size(window_size)
                .with_resizable(false)
                .with_maximize_button(false),
            move |ctx, _class| {
                let send_window_top = config_handle.lock().unwrap().update;
                if send_window_top {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    config_handle.lock().unwrap().update = false;
                }
                let mut config_window = config_handle.lock().unwrap();
                if ctx.input(|i| i.viewport().close_requested()) {
                    config_window.show = false;
                }

                config_window.ui(ctx);
            },
        );
    }

    pub fn ui(&mut self, ctx: &Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(format!("{}", get_model_name()));
            ui.separator();

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_tab, "Device".into(), "Device");
                ui.selectable_value(&mut self.current_tab, "Key Mapping".into(), "Key Mapping");
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
}

pub fn load_settings_icon() -> egui::IconData {
    let size = SETTINGS_ICON_SIZE;
    let mut pixmap = tiny_skia::Pixmap::new(size, size).unwrap();

    let opt = usvg::Options::default();

    let coloured_svg = include_str!("../../assets/settings_icon.svg")
        .replace("#FFFFFF", SETTINGS_ICON_COLOR)
        .replace("#ffffff", &SETTINGS_ICON_COLOR.to_lowercase());
    let tree = usvg::Tree::from_str(&coloured_svg, &opt).expect("Failed to parse SVG");

    let svg_size = tree.size();
    let scale = (size as f32 / svg_size.width()).min(size as f32 / svg_size.height());

    let tx = (size as f32 - (svg_size.width() * scale)) / 2.0;
    let ty = (size as f32 - (svg_size.height() * scale)) / 2.0;

    let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let rgba = pixmap.take(); // Returns the raw Vec<u8>

    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

fn get_model_name() -> String {
    app().device.get_model_name().unwrap_or_default()
}
