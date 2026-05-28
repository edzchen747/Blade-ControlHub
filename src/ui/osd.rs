//! OSD (On-Screen Display) overlay for visual feedback.
//!
//! Handles content management, styling, and rendering of the transient
//! notification panel. Animation state is delegated to `OsdAnimation`.

use eframe::egui;

use crate::ui::app_events::OsdResponse;
use crate::ui::icons::OsdIconId;
use crate::ui::layout;
use crate::ui::osd_animation::{OsdAnimation, OsdState};
use crate::ui::theme;

// ── Osd ─────────────────────────────────────────────────────────────────────

/// The OSD overlay: manages content, delegates animation to [`OsdAnimation`].
pub struct Osd {
    animation: OsdAnimation,

    // Current OSD content
    osd_text: String,
    osd_icon_id: Option<OsdIconId>,
    osd_total_levels: u8,
    osd_current_level: u8,
}

impl Osd {
    pub fn new() -> Self {
        Self {
            animation: OsdAnimation::new(),
            osd_text: "Blade ControlHub".to_string(),
            osd_icon_id: None,
            osd_total_levels: 0,
            osd_current_level: 0,
        }
    }

    pub fn run(&mut self, ctx: &egui::Context, trigger_osd: bool) {
        if trigger_osd {
            self.animation.trigger(ctx);
        } else if !self.animation.is_visible() {
            return;
        }

        let previous_state = self.animation.state();
        self.animation.advance(ctx);
        let current_state = self.animation.state();
        // Move OSD window off screen and shrink - prevents stuttering in games
        if previous_state != current_state {
            match current_state {
                OsdState::Hidden => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                        -3000.0, -3000.0,
                    )));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1.0, 1.0)));
                }
                OsdState::FadingIn => {
                    self.animation.is_onscreen = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                        layout::OSD_WINDOW_SIZE,
                    ));
                }
                _ => {}
            }
        }

        self.animation.center_viewport_if_needed(ctx);
        self.render_osd(ctx);
    }

    /// Applies an `OsdResponse` to the current OSD display state.
    pub fn apply_osd_response(&mut self, response: OsdResponse) {
        self.osd_text = response.text;
        self.osd_icon_id = response.icon_id;
        self.osd_total_levels = response.total_levels;
        self.osd_current_level = response.current_level;
    }

    // ── OSD Rendering ───────────────────────────────────────────────────

    /// Renders the OSD overlay content (icon, text, slider bar).
    fn render_osd(&self, ctx: &egui::Context) {
        if self.animation.state() == OsdState::Hidden {
            return;
        }

        let colors = theme::OsdColors::with_alpha(self.animation.fade_alpha());

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(colors.background)
                    .rounding(layout::OSD_ROUNDING)
                    .stroke(egui::Stroke::new(layout::OSD_BORDER_WIDTH, colors.accent))
                    .outer_margin(layout::OSD_OUTER_MARGIN)
                    .inner_margin(layout::OSD_INNER_MARGIN)
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.add_space(layout::OSD_CONTENT_TOP_SPACING);
                            self.render_icon(ui);
                            ui.add_space(layout::OSD_ICON_TEXT_SPACING);
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
            let transparency_mask =
                egui::Color32::WHITE.linear_multiply(self.animation.fade_alpha());
            ui.add_space(layout::OSD_ICON_TOP_SPACING);
            ui.add(
                image
                    .max_size(layout::ICON_MAX_SIZE)
                    .tint(transparency_mask),
            );
        } else {
            ui.add_space(layout::OSD_PLACEHOLDER_ICON_HEIGHT);
        }
    }

    /// Renders the OSD text label if non-empty.
    fn render_text(&self, ui: &mut egui::Ui, colors: &theme::OsdColors) {
        if !self.osd_text.is_empty() {
            ui.label(
                egui::RichText::new(self.osd_text.clone())
                    .color(colors.text)
                    .size(layout::OSD_TEXT_FONT_SIZE),
            );
        } else {
            ui.add_space(layout::OSD_PLACEHOLDER_TEXT_HEIGHT);
        }
    }

    /// Renders the segmented slider bar for level-based OSD indicators.
    fn render_slider_bar(&self, ui: &mut egui::Ui, colors: &theme::OsdColors) {
        if self.osd_total_levels == 0 {
            return;
        }

        let segment_width = (layout::SLIDER_BAR_WIDTH
            - (layout::SLIDER_BAR_SPACING * (self.osd_total_levels - 1) as f32))
            / self.osd_total_levels as f32;

        let content_rect = ui.available_rect_before_wrap();
        let center_x = content_rect.center().x;
        let current_y = ui.cursor().top();
        let start_x = center_x - (layout::SLIDER_BAR_WIDTH / 2.0);

        for i in 0..self.osd_total_levels {
            let x = start_x + (i as f32 * (segment_width + layout::SLIDER_BAR_SPACING));
            let rect = egui::Rect::from_min_size(
                egui::pos2(x, current_y),
                egui::vec2(segment_width, layout::SLIDER_BAR_HEIGHT),
            );

            let color = if i < self.osd_current_level {
                colors.accent
            } else {
                colors.accent.linear_multiply(layout::SLIDER_INACTIVE_ALPHA)
            };

            ui.painter().rect_filled(rect, 1.0, color);
        }
    }
}
