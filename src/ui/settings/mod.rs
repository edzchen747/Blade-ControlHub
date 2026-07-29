use eframe::egui::{self};
use egui::Context;
use resvg::{tiny_skia, usvg};

use crate::razer::config::AppConfig;
use crate::ui::custom_key_map::CustomKeyMap;
use crate::ui::theme::{
    SETTINGS_ICON_COLOR, SETTINGS_ICON_SIZE, SETTINGS_PADDING_RATIO, SETTINGS_WINDOW_SIZE,
    SETTINGS_WINDOW_TITLE,
};

mod appearance_tab;
mod device_tab;
mod key_mapping_tab;
pub mod store;

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

    // Note: The redundant pub fn run(...) has been removed entirely!

    pub fn ui(&mut self, ctx: &Context) {
        // Force focus if the tray requested an update pop-to-front
        if self.update {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.update = false;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(""); // get_model_name(&self.app_config)
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
        let mut pixmap = tiny_skia::Pixmap::new(size, size).unwrap();
        let opt = usvg::Options::default();

        let coloured_svg = include_str!("../../../assets/settings_icon.svg")
            .replace("#FFFFFF", SETTINGS_ICON_COLOR)
            .replace("#ffffff", &SETTINGS_ICON_COLOR.to_lowercase());
        let tree = usvg::Tree::from_str(&coloured_svg, &opt).expect("Failed to parse SVG");

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

    fn get_model_name(app_config: &Option<AppConfig>) -> String {
        if let Some(config) = app_config {
            // Replace `.model_name` with whatever field name your AppConfig struct
            // uses to store the hardware identifier string (e.g., config.device_name)
            config.model_name.clone()
        } else {
            // Fallback safely if config isn't populated yet
            "Blade Laptop".to_string()
        }
    }
}
