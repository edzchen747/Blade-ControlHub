//! OSD fade animation state machine and viewport centering logic.
//!
//! Isolates the animation timing, state transitions, and viewport positioning
//! from the OSD rendering and content management layers.

use eframe::egui;
use std::time::{Duration, Instant};

use crate::ui::layout;

/// Tracks the current visibility phase of the OSD overlay.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum OsdState {
    Hidden,
    FadingIn,
    Active,
    FadingOut,
}

/// Self-contained animation controller for the OSD overlay.
/// Handles fade-in/fade-out timing, state transitions, and viewport centering.
#[derive(Debug)]
pub struct OsdAnimation {
    state: OsdState,
    show_until: Option<Instant>,
    is_centered: bool,
    fade_alpha: f32,
    last_update: Instant,
    pub is_onscreen: bool,
}

impl OsdAnimation {
    pub fn new() -> Self {
        Self {
            state: OsdState::Hidden,
            show_until: None,
            is_centered: false,
            fade_alpha: 0.0,
            last_update: Instant::now(),
            is_onscreen: true,
        }
    }

    /// Returns the current visibility state.
    #[inline]
    pub fn state(&self) -> OsdState {
        self.state
    }

    /// Returns the current fade alpha (0.0 = invisible, ~0.9 = fully visible).
    #[inline]
    pub fn fade_alpha(&self) -> f32 {
        self.fade_alpha
    }

    /// Whether the OSD is currently visible (fading in, active, or fading out).
    #[inline]
    pub fn is_visible(&self) -> bool {
        !matches!(self.state, OsdState::Hidden)
    }

    /// Initiates the OSD fade-in and resets the display timer.
    pub fn trigger(&mut self, ctx: &egui::Context) {
        self.state = OsdState::FadingIn;
        self.show_until =
            Some(Instant::now() + Duration::from_millis(layout::OSD_DISPLAY_DURATION_MS));
        self.last_update = Instant::now();
        self.is_centered = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
        ctx.request_repaint();
    }

    /// Advances the animation by one frame. Call this each frame.
    pub fn advance(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        // Check if Active state should transition to FadingOut
        if self.state == OsdState::Active
            && let Some(timeout) = self.show_until
        {
            if now >= timeout {
                self.state = OsdState::FadingOut;
                self.show_until = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));
                ctx.request_repaint();
                return;
            } else {
                ctx.request_repaint_after(timeout.duration_since(now));
            }
        }

        self.update_alpha(ctx, dt);
    }

    /// Centers the OSD window on the viewport. Call after the first frame of FadingIn/Active.
    pub fn center_viewport_if_needed(&mut self, ctx: &egui::Context) {
        if self.is_centered {
            return;
        }
        if !matches!(self.state, OsdState::FadingIn | OsdState::Active) {
            return;
        }

        if let Some(screen_size) = ctx.input(|i| i.viewport().monitor_size) {
            let x = (screen_size.x - layout::OSD_WINDOW_SIZE.x) * 0.5;
            let y = (screen_size.y - layout::OSD_WINDOW_SIZE.y) * layout::OSD_POSITION_Y_RATIO;
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(layout::OSD_WINDOW_SIZE));
            self.is_centered = true;
        }
    }

    /// Updates the fade alpha based on the current state and delta time.
    fn update_alpha(&mut self, ctx: &egui::Context, dt: f32) {
        let target_alpha = match self.state {
            OsdState::FadingIn | OsdState::Active => layout::TARGET_ALPHA_VISIBLE,
            OsdState::FadingOut | OsdState::Hidden => 0.0,
        };

        let diff = target_alpha - self.fade_alpha;
        if diff.abs() > layout::FADE_EPSILON {
            let speed = if self.state == OsdState::FadingIn {
                layout::FADE_IN_SPEED
            } else {
                layout::FADE_OUT_SPEED
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
}

impl Default for OsdAnimation {
    fn default() -> Self {
        Self::new()
    }
}
