use std::sync::OnceLock;

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use tray_icon::TrayIcon;

use crate::ui::app_events::*;
use crate::ui::icon;
use crate::ui::osd::*;

pub static TRAY_APP_TX: OnceLock<Sender<AppEvent>> = OnceLock::new();
pub static OSD_CONTEXT: OnceLock<egui::Context> = OnceLock::new();

pub trait OnceLockExt<T> {
    fn get_or_timeout(&self) -> Option<T>
    where
        T: Clone;
}

impl<T> OnceLockExt<T> for OnceLock<T> {
    /// Attempts to get the value, polling until the timeout is reached
    fn get_or_timeout(&self) -> Option<T>
    where
        T: Clone,
    {
        if let Some(v) = self.get() {
            return Some(v.clone());
        }

        let start_time = Instant::now();
        let timeout = Duration::from_millis(500);
        let poll_interval = Duration::from_millis(50);

        loop {
            thread::sleep(poll_interval);

            if let Some(v) = self.get() {
                return Some(v.clone());
            }

            if start_time.elapsed() >= timeout {
                return None;
            }
        }
    }
}

struct TrayApp {
    state: OSDState,
    show_until: Option<Instant>,
    is_centered: bool,
    fade_alpha: f32,
    rx: Receiver<AppEvent>,
    tray_icon: TrayIcon,
    osd_text: String,
    osd_icon_id: Option<OsdIconId>,
    osd_total_levels: u8,
    osd_curr_level: u8,
    last_update: Instant,
}

impl eframe::App for TrayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Poll event signals ---
        while let Ok(event) = self.rx.try_recv() {
            let trigger_osd = process_event(
                event,
                &mut self.tray_icon,
                &mut self.osd_text,
                &mut self.osd_icon_id,
                &mut self.osd_total_levels,
                &mut self.osd_curr_level,
            );
            if trigger_osd {
                self.trigger_osd(ctx);
            } else {
                if self.state == OSDState::Hidden {
                    return;
                }
            }
        }

        // --- Animation Logic ---
        let target_alpha = match self.state {
            OSDState::FadingIn | OSDState::Active => 0.9,
            OSDState::FadingOut | OSDState::Hidden => 0.0,
        };

        if self.state == OSDState::Active {
            if let Some(timeout) = self.show_until {
                let now = Instant::now();
                if now >= timeout {
                    self.state = OSDState::FadingOut;
                    self.show_until = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));
                    ctx.request_repaint();
                } else {
                    ctx.request_repaint_after(
                        timeout.duration_since(now) + Duration::from_millis(16),
                    );
                }
            }
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        let target_diff = target_alpha - self.fade_alpha;

        if target_diff.abs() > 0.001 {
            // Alpha change per second
            // 2.0 means it takes 0.5s to go from 0.0 to 1.0
            let change_per_sec = if self.state == OSDState::FadingIn {
                255.0
            } else {
                2.0
            };

            // Move towards target at a constant rate
            let step = change_per_sec * dt;

            if target_diff > 0.0 {
                self.fade_alpha = (self.fade_alpha + step).min(target_alpha);
            } else {
                self.fade_alpha = (self.fade_alpha - step).max(target_alpha);
            }

            ctx.request_repaint(); // Request next frame immediately for smooth motion
        } else {
            self.fade_alpha = target_alpha;
            if self.state == OSDState::FadingIn {
                self.state = OSDState::Active;
            }
            if self.state == OSDState::FadingOut {
                self.state = OSDState::Hidden;
            }
        }

        // --- Centering ---
        if (self.state == OSDState::FadingIn || self.state == OSDState::Active) && !self.is_centered
        {
            if let Some(screen_size) = ctx.input(|i| i.viewport().monitor_size) {
                let window_size = egui::vec2(220.0, 220.0);
                let x = (screen_size.x - window_size.x) * 0.5;
                let y = (screen_size.y - window_size.y) * 0.85;
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(window_size));
                self.is_centered = true;
            }
        }

        // --- Rendering ---
        if self.fade_alpha > 0.001 {
            let a = self.fade_alpha;
            let bg = egui::Color32::from_rgba_premultiplied(
                (30. * a) as u8,
                (30. * a) as u8,
                (30. * a) as u8,
                (230. * a) as u8,
            );
            let gold = egui::Color32::from_rgba_premultiplied(
                (255. * a) as u8,
                (215. * a) as u8,
                (0. * a) as u8,
                (230. * a) as u8,
            );
            let text = egui::Color32::from_rgba_premultiplied(
                (255. * a) as u8,
                (255. * a) as u8,
                (255. * a) as u8,
                (230. * a) as u8,
            );

            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
                .show(ctx, |ui| {
                    egui::Frame::default()
                        .fill(bg)
                        .rounding(12.0)
                        .stroke(egui::Stroke::new(2.0, gold))
                        .outer_margin(10.0)
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.set_min_size(ui.available_size());
                            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                                ui.add_space(10.0); // Top padding

                                // Icon Rendering
                                if let Some(id) = &self.osd_icon_id {
                                    let (name, bytes) = get_icon_data(id);
                                    let transparency_mask =
                                        egui::Color32::WHITE.linear_multiply(self.fade_alpha);
                                    ui.add_space(20.0);
                                    ui.add(
                                        egui::Image::from_bytes(name, bytes)
                                            .max_size(egui::vec2(80.0, 80.0))
                                            .tint(transparency_mask),
                                    );
                                } else {
                                    ui.add_space(60.0);
                                }

                                ui.add_space(8.0);

                                // Text Label
                                if self.osd_text.len() > 0 {
                                    ui.label(
                                        egui::RichText::new(self.osd_text.clone())
                                            .color(text)
                                            .size(27.0),
                                    );
                                } else {
                                    ui.add_space(32.0);
                                }

                                // Slider Bar
                                if self.osd_total_levels != 0 {
                                    let spacing = 2.0;
                                    let total_bar_width = 160.0; // fixed width
                                    let rect_height = 8.0;

                                    let rect_width = (total_bar_width
                                        - (spacing * (self.osd_total_levels - 1) as f32))
                                        / self.osd_total_levels as f32;

                                    // Get the center of the current UI container (the 200x200 square)
                                    let content_rect = ui.available_rect_before_wrap();
                                    let center_x = content_rect.center().x;
                                    let current_y = ui.cursor().top(); // Where we currently are vertically

                                    let start_x = center_x - (total_bar_width / 2.0);

                                    for i in 0..self.osd_total_levels {
                                        let x = start_x + (i as f32 * (rect_width + spacing));
                                        let rect = egui::Rect::from_min_size(
                                            egui::pos2(x, current_y),
                                            egui::vec2(rect_width, rect_height),
                                        );

                                        let color = if i < self.osd_curr_level {
                                            gold
                                        } else {
                                            gold.linear_multiply(0.02)
                                        };

                                        ui.painter().rect_filled(rect, 1.0, color);
                                    }
                                }
                            });
                        });
                });
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

impl TrayApp {
    fn trigger_osd(&mut self, ctx: &egui::Context) {
        self.state = OSDState::FadingIn;
        self.show_until = Some(Instant::now() + Duration::from_millis(1500));
        self.is_centered = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
    }
}

pub struct TrayAppHandle {
    tx: Sender<AppEvent>,
}

impl TrayAppHandle {
    pub fn send(&self, cmd: AppEvent) {
        self.tx
            .send(cmd)
            .expect("Fatal internal error: OSD TX send");
        OSD_CONTEXT
            .get()
            .expect("Fatal internal error: OSD Context get error")
            .request_repaint();
    }
}

pub fn run() {
    let (tx, rx) = mpsc::channel::<AppEvent>();
    eframe::run_native(
        "Blade ControlHub OSD",
        options(),
        Box::new(move |cc| {
            TRAY_APP_TX
                .set(tx)
                .expect("Fatal internal error: OSD channel initialize error");
            OSD_CONTEXT
                .set(cc.egui_ctx.clone())
                .expect("Fatal internal error: OSD Context initialize error");
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Box::new(TrayApp {
                state: OSDState::Hidden,
                show_until: None,
                is_centered: false,
                fade_alpha: 0.0,
                rx,
                tray_icon: icon::TrayIcon::new(),
                osd_text: "Blade ControlHub".to_string(),
                osd_icon_id: None,
                osd_total_levels: 0,
                osd_curr_level: 0,
                last_update: Instant::now(),
            })
        }),
    )
    .expect("Fatal internal error: OSD error");
}

pub fn tray_app() -> TrayAppHandle {
    let tx = TRAY_APP_TX
        .get_or_timeout()
        .expect("Fatal internal error: TrayApp channel initialization timeout");
    TrayAppHandle { tx: tx.clone() }
}
