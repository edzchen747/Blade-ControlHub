use std::{
    sync::{Arc, Mutex},
    time,
};

use eframe::egui::{self, Ui};
use egui::Context;
use resvg::{tiny_skia, usvg};

use crate::{
    razer::{config::AppConfig, device_handle::device},
    ui::{app::app, app_events::OsdEvent},
};

#[allow(dead_code)]
pub struct CustomKeyMap {
    #[allow(dead_code)]
    func_keys: Vec<FuncKeyMap>,
    razer_keys: Vec<RazerKeyMap>,
    pub listening_idx: Option<usize>,
    pub special_key: Option<u8>,
}

impl Default for CustomKeyMap {
    fn default() -> Self {
        Self {
            func_keys: vec![FuncKeyMap::default()],
            razer_keys: vec![RazerKeyMap::default()],
            listening_idx: None,
            special_key: None,
        }
    }
}

#[allow(dead_code)]
#[derive(Default, Clone)]
struct FuncKeyMap {
    key: String,
    action: String,
}

#[derive(Default, Clone)]
struct RazerKeyMap {
    key_code: u8,
    name: String,
    action: String,
}

pub struct Settings {
    pub show: bool,
    pub update: bool,
    current_tab: String,
    key_map_current_tab: String,
    app_config: Option<AppConfig>,
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
            custom_key_map: CustomKeyMap::default(),
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
            ui.heading("Settings");
            ui.separator();

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_tab, "Device".into(), "Device");
                ui.selectable_value(&mut self.current_tab, "Key Mapping".into(), "Key Mapping");
            });
            ui.separator();

            ui.add_space(20.0);

            match self.current_tab.as_str() {
                "Device" => {
                    self.device_tab(ui, ctx);
                }
                "Key Mapping" => {
                    self.key_mapping_tab(ui, ctx);
                }
                "Appearance" => {
                    ui.label("Text Color:");
                    ui.color_edit_button_rgba_unmultiplied(&mut [1.0, 2.0, 3.0, 4.0]);
                }
                _ => {}
            }
        });
    }

    fn device_tab(&mut self, ui: &mut Ui, ctx: &Context) {
        self.default_func_key_switcher(ui, ctx);
    }

    fn default_func_key_switcher(&mut self, ui: &mut Ui, ctx: &Context) {
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
                app().send(OsdEvent::ToggleDefaultMultimediaKeys(mode).into());
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        });
    }

    fn key_mapping_tab(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.key_map_current_tab,
                "Multimedia Keys".into(),
                "Function Keys",
            );
            ui.selectable_value(
                &mut self.key_map_current_tab,
                "Razer Special Keys".into(),
                "Razer Special Keys",
            );
        });
        ui.separator();

        match self.key_map_current_tab.as_str() {
            "Multimedia Keys" => self.multimedia_key_tab(ui, ctx),
            "Razer Special Keys" => self.razer_special_key_tab(ui, ctx),
            _ => {}
        }
    }

    fn multimedia_key_tab(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.label("Remap F1 - F12 Keys when default behaviour is set to multimedia");
        ui.add_space(20.0);
        self.default_func_key_switcher(ui, ctx);
    }

    fn razer_special_key_tab(&mut self, ui: &mut Ui, ctx: &Context) {
        if let Some(idx) = self.custom_key_map.listening_idx {
            ctx.request_repaint_after(time::Duration::from_millis(200));
            if let Some(key_code) = self.custom_key_map.special_key {
                reset_value(&mut self.custom_key_map.razer_keys, key_code);
                if let Some(row) = self.custom_key_map.razer_keys.get_mut(idx) {
                    row.key_code = key_code;
                    self.custom_key_map.listening_idx = None;
                    self.custom_key_map.special_key = None;
                }
            }

            // Allow cancelling with Escape
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.custom_key_map.listening_idx = None;
            }
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut row_to_remove = None;

            for (idx, row) in self.custom_key_map.razer_keys.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}:", idx + 1));
                    ui.add(
                        egui::TextEdit::singleline(&mut row.name)
                            .hint_text("Key label")
                            .desired_width(120.0), // Width in points
                    );
                    let is_this_row_listening = self.custom_key_map.listening_idx == Some(idx);
                    let btn_label = if is_this_row_listening {
                        "Press key...".into()
                    } else {
                        if row.key_code == 0 {
                            "None".to_string()
                        } else {
                            format!("{:#04X}", row.key_code)
                        }
                    };

                    if ui
                        .add_sized([60.0, 20.0], egui::Button::new(btn_label))
                        .clicked()
                    {
                        self.custom_key_map.listening_idx = Some(idx);
                    }
                    ui.label("➡");
                    ui.add(
                        egui::TextEdit::singleline(&mut row.action)
                            .hint_text("Action")
                            .desired_width(120.0), // Width in points
                    );
                    if ui.button("🗑").clicked() {
                        row_to_remove = Some(idx);
                    }
                });
                ui.add_space(10.0);
            }

            if let Some(idx) = row_to_remove {
                self.custom_key_map.razer_keys.remove(idx);
            }

            ui.add_space(10.0);
            ui.separator();

            let can_add = self.custom_key_map.listening_idx.is_none()
                && self
                    .custom_key_map
                    .razer_keys
                    .iter()
                    .all(|last| !last.name.is_empty() && last.key_code != 0);

            ui.add_enabled_ui(can_add, |ui| {
                if ui.button("➕ Add New Row").clicked() {
                    self.custom_key_map.razer_keys.push(RazerKeyMap::default());
                }
            });

            if !can_add {
                ui.weak("Complete current row to add more");
            }
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

fn reset_value(vec: &mut [RazerKeyMap], key_code: u8) {
    for key_map in vec.iter_mut() {
        if key_map.key_code == key_code {
            key_map.key_code = 0;
        }
    }
}
