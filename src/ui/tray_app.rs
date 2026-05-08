use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui;
use tray_icon::TrayIcon;

use crate::ui::app_events::*;
use crate::ui::icon;
use crate::ui::osd::*;
use crate::utils::oncelock_ext::OnceLockExt;

// ── Global State ────────────────────────────────────────────────────────────

static TRAY_APP_TX: OnceLock<Sender<AppEvent>> = OnceLock::new();
static OSD_CONTEXT: OnceLock<egui::Context> = OnceLock::new();

// ── Fade Animation ──────────────────────────────────────────────────────────

const FADE_IN_SPEED: f32 = 255.0;
const FADE_OUT_SPEED: f32 = 2.0;
const FADE_EPSILON: f32 = 0.001;
const TARGET_ALPHA_VISIBLE: f32 = 0.9;

// ── TrayApp (eframe application) ────────────────────────────────────────────

struct TrayApp {
    // OSD animation state
    state: OsdState,
    show_until: Option<Instant>,
    is_centered: bool,
    fade_alpha: f32,
    last_update: Instant,

    // Event channel
    rx: Receiver<AppEvent>,

    // System tray
    tray_icon: TrayIcon,

    // Current OSD content
    osd_text: String,
    osd_icon_id: Option<OsdIconId>,
    osd_total_levels: u8,
    osd_current_level: u8,
}

impl TrayApp {
    fn new(rx: Receiver<AppEvent>) -> Self {
        Self {
            state: OsdState::Hidden,
            show_until: None,
            is_centered: false,
            fade_alpha: 0.0,
            last_update: Instant::now(),
            rx,
            tray_icon: icon::build_tray_icon(),
            osd_text: "Blade ControlHub".to_string(),
            osd_icon_id: None,
            osd_total_levels: 0,
            osd_current_level: 0,
        }
    }

    // ── Event Handling ──────────────────────────────────────────────────

    /// Drains pending events and returns whether the OSD should be shown.
    fn poll_events(&mut self) -> bool {
        let mut should_trigger = false;
        while let Ok(event) = self.rx.try_recv() {
            if let Some(response) = process_event(event, &mut self.tray_icon) {
                self.apply_osd_response(response);
                should_trigger = true;
            }
        }
        should_trigger
    }

    /// Applies an `OsdResponse` to the current OSD display state.
    fn apply_osd_response(&mut self, response: OsdResponse) {
        self.osd_text = response.text;
        self.osd_icon_id = response.icon_id;
        self.osd_total_levels = response.total_levels;
        self.osd_current_level = response.current_level;
    }

    /// Initiates the OSD fade-in and resets the display timer.
    fn trigger_osd(&mut self, ctx: &egui::Context) {
        self.state = OsdState::FadingIn;
        self.show_until = Some(Instant::now() + Duration::from_millis(OSD_DISPLAY_DURATION_MS));
        self.is_centered = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
    }

    // ── Animation ───────────────────────────────────────────────────────

    /// Advances the fade animation state machine and returns the current alpha.
    fn advance_animation(&mut self, ctx: &egui::Context, dt: f32) {
        let target_alpha = match self.state {
            OsdState::FadingIn | OsdState::Active => TARGET_ALPHA_VISIBLE,
            OsdState::FadingOut | OsdState::Hidden => 0.0,
        };

        // Check if the active display timer has expired
        if self.state == OsdState::Active {
            if let Some(timeout) = self.show_until {
                let now = Instant::now();
                if now >= timeout {
                    self.state = OsdState::FadingOut;
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

        // Smoothly interpolate alpha towards the target
        let diff = target_alpha - self.fade_alpha;
        if diff.abs() > FADE_EPSILON {
            let speed = if self.state == OsdState::FadingIn {
                FADE_IN_SPEED
            } else {
                FADE_OUT_SPEED
            };
            let step = speed * dt;

            if diff > 0.0 {
                self.fade_alpha = (self.fade_alpha + step).min(target_alpha);
            } else {
                self.fade_alpha = (self.fade_alpha - step).max(target_alpha);
            }
            ctx.request_repaint();
        } else {
            self.fade_alpha = target_alpha;
            match self.state {
                OsdState::FadingIn => self.state = OsdState::Active,
                OsdState::FadingOut => self.state = OsdState::Hidden,
                _ => {}
            }
        }
    }

    // ── Viewport Centering ──────────────────────────────────────────────

    /// Centers the OSD window on screen if it hasn't been centered yet.
    fn center_viewport_if_needed(&mut self, ctx: &egui::Context) {
        if self.is_centered {
            return;
        }
        if !matches!(self.state, OsdState::FadingIn | OsdState::Active) {
            return;
        }

        if let Some(screen_size) = ctx.input(|i| i.viewport().monitor_size) {
            let x = (screen_size.x - OSD_WINDOW_SIZE.x) * 0.5;
            let y = (screen_size.y - OSD_WINDOW_SIZE.y) * OSD_POSITION_Y_RATIO;
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(OSD_WINDOW_SIZE));
            self.is_centered = true;
        }
    }

    // ── OSD Rendering ───────────────────────────────────────────────────

    /// Renders the OSD overlay content (icon, text, slider bar).
    fn render_osd(&self, ctx: &egui::Context) {
        if self.fade_alpha <= FADE_EPSILON {
            return;
        }

        let colors = OsdColors::with_alpha(self.fade_alpha);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(colors.background)
                    .rounding(OSD_ROUNDING)
                    .stroke(egui::Stroke::new(OSD_BORDER_WIDTH, colors.accent))
                    .outer_margin(OSD_OUTER_MARGIN)
                    .inner_margin(OSD_INNER_MARGIN)
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.add_space(10.0);
                            self.render_icon(ui);
                            ui.add_space(8.0);
                            self.render_text(ui, &colors);
                            self.render_slider_bar(ui, &colors);
                        });
                    });
            });
    }

    /// Renders the OSD icon if one is set.
    fn render_icon(&self, ui: &mut egui::Ui) {
        if let Some(id) = &self.osd_icon_id {
            let (name, bytes) = id.icon_data();
            let image = match bytes {
                std::borrow::Cow::Borrowed(b) => egui::Image::from_bytes(name, b),
                std::borrow::Cow::Owned(v) => egui::Image::from_bytes(name, v),
            };
            let transparency_mask = egui::Color32::WHITE.linear_multiply(self.fade_alpha);
            ui.add_space(20.0);
            ui.add(image.max_size(ICON_MAX_SIZE).tint(transparency_mask));
        } else {
            ui.add_space(60.0);
        }
    }

    /// Renders the OSD text label if non-empty.
    fn render_text(&self, ui: &mut egui::Ui, colors: &OsdColors) {
        if !self.osd_text.is_empty() {
            ui.label(
                egui::RichText::new(self.osd_text.clone())
                    .color(colors.text)
                    .size(TEXT_FONT_SIZE),
            );
        } else {
            ui.add_space(32.0);
        }
    }

    /// Renders the segmented slider bar for level-based OSD indicators.
    fn render_slider_bar(&self, ui: &mut egui::Ui, colors: &OsdColors) {
        if self.osd_total_levels == 0 {
            return;
        }

        let segment_width = (SLIDER_BAR_WIDTH
            - (SLIDER_BAR_SPACING * (self.osd_total_levels - 1) as f32))
            / self.osd_total_levels as f32;

        let content_rect = ui.available_rect_before_wrap();
        let center_x = content_rect.center().x;
        let current_y = ui.cursor().top();
        let start_x = center_x - (SLIDER_BAR_WIDTH / 2.0);

        for i in 0..self.osd_total_levels {
            let x = start_x + (i as f32 * (segment_width + SLIDER_BAR_SPACING));
            let rect = egui::Rect::from_min_size(
                egui::pos2(x, current_y),
                egui::vec2(segment_width, SLIDER_BAR_HEIGHT),
            );

            let color = if i < self.osd_current_level {
                colors.accent
            } else {
                colors.accent.linear_multiply(0.02)
            };

            ui.painter().rect_filled(rect, 1.0, color);
        }
    }
}

// ── eframe::App Implementation ──────────────────────────────────────────────

impl eframe::App for TrayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let should_trigger = self.poll_events();

        if should_trigger {
            self.trigger_osd(ctx);
        } else if self.state == OsdState::Hidden {
            return;
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        self.advance_animation(ctx, dt);
        self.center_viewport_if_needed(ctx);
        self.render_osd(ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

// ── Public Handle ───────────────────────────────────────────────────────────

/// A clonable handle for sending `AppEvent`s to the OSD application.
pub struct TrayAppHandle {
    tx: Sender<AppEvent>,
}

impl TrayAppHandle {
    pub fn send(&self, event: AppEvent) {
        self.tx
            .send(event)
            .expect("Fatal internal error: OSD TX send");
        OSD_CONTEXT
            .get()
            .expect("Fatal internal error: OSD Context get error")
            .request_repaint();
    }
}

/// Returns a handle to the running `TrayApp` event channel.
pub fn tray_app() -> TrayAppHandle {
    let tx = TRAY_APP_TX
        .get_or_timeout()
        .expect("Fatal internal error: TrayApp channel initialization timeout");
    TrayAppHandle { tx: tx.clone() }
}

// ── Entry Point ─────────────────────────────────────────────────────────────

/// Launches the eframe-based OSD overlay and tray icon application.
pub fn run() {
    let (tx, rx) = mpsc::channel::<AppEvent>();
    eframe::run_native(
        "Blade ControlHub OSD",
        native_options(),
        Box::new(move |cc| {
            TRAY_APP_TX
                .set(tx)
                .expect("Fatal internal error: OSD channel initialize error");
            OSD_CONTEXT
                .set(cc.egui_ctx.clone())
                .expect("Fatal internal error: OSD Context initialize error");
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Box::new(TrayApp::new(rx))
        }),
    )
    .expect("Fatal internal error: OSD error");
}
