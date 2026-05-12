use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::Context;
use resvg::{tiny_skia, usvg};

use crate::{
    razer::{config::AppConfig, device_handle::device},
    ui::{app_events::OsdEvent, tray_app::tray_app},
};

pub struct Settings {
    pub show: bool,
    current_tab: String,
    app_config: Option<AppConfig>,
}

impl Settings {
    pub fn new() -> Self {
        Self {
            show: false,
            current_tab: "Device".to_string(),
            app_config: None,
        }
    }

    pub fn show(&mut self, config: AppConfig) {
        self.app_config = Some(config);
        self.show = true;
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

        let window_size = egui::vec2(450.0, 600.0);
        let padding = screen_height * 0.1;
        let spawn_pos = egui::pos2(
            screen_width - window_size.x - padding * 0.1,
            screen_height - window_size.y - padding,
        );

        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("settings_window"),
            egui::ViewportBuilder::default()
                .with_title("Blade ControlHub")
                .with_icon(icon_data.clone())
                .with_mouse_passthrough(false)
                .with_position(spawn_pos)
                .with_inner_size(window_size)
                .with_min_inner_size(window_size)
                .with_max_inner_size(window_size)
                .with_resizable(false)
                .with_maximize_button(false),
            move |ctx, _class| {
                let mut config_window = config_handle.lock().unwrap();
                if ctx.input(|i| i.viewport().close_requested()) {
                    config_window.show = false;
                }

                config_window.ui(ctx);
                if ctx.input(|i| i.pointer.any_down() || !i.events.is_empty()) {
                    ctx.request_repaint();
                }
            },
        );
    }

    pub fn ui(&mut self, ctx: &Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Settings");
            ui.separator();

            // Tab selection logic
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_tab, "Device".into(), "Device");
                ui.selectable_value(&mut self.current_tab, "Appearance".into(), "Appearance");
            });
            ui.separator();

            ui.add_space(20.0);

            // Settings controls
            match self.current_tab.as_str() {
                "Device" => {
                    // ui.label("Display Message:");
                    // ui.text_edit_singleline(&mut "Item1");
                    // ui.add(egui::Slider::new(&mut 5, 100..=5000).text("Update Rate (ms)"));

                    let mut changed = false;
                    ui.label("Default Function key behaviour:");
                    ui.horizontal(|ui| {
                        changed |= ui
                            .selectable_value(
                                &mut self.app_config.as_mut().unwrap().default_multimedia_keys,
                                false,
                                "Function",
                            )
                            .changed();
                        changed |= ui
                            .selectable_value(
                                &mut self.app_config.as_mut().unwrap().default_multimedia_keys,
                                true,
                                "Multimedia",
                            )
                            .changed();
                        if changed {
                            let mode = device().toggle_default_multimedia_keys();
                            tray_app().send(OsdEvent::ToggleDefaultMultimediaKeys(mode));
                            ctx.request_repaint_of(egui::ViewportId::ROOT);
                        }
                    });
                }
                "Appearance" => {
                    ui.label("Text Color:");
                    ui.color_edit_button_rgba_unmultiplied(&mut [1.0, 2.0, 3.0, 4.0]);
                }
                _ => {}
            }

            // ui.add_space(20.0);
            // if ui.button("Close Window").clicked() {
            //     self.show = false;
            // }
        });
    }
}

pub fn load_settings_icon() -> egui::IconData {
    let size = 64;
    let mut pixmap = tiny_skia::Pixmap::new(size, size).unwrap();

    let opt = usvg::Options::default();

    let coloured_svg = include_str!("../../assets/settings_icon.svg")
        .replace("#FFFFFF", "#FFD700")
        .replace("#ffffff", &"#FFD700".to_lowercase());
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
