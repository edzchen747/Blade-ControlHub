use eframe::{NativeOptions, egui};

// ── OSD Constants ───────────────────────────────────────────────────────────

pub const OSD_WINDOW_SIZE: egui::Vec2 = egui::vec2(220.0, 220.0);
pub const OSD_DISPLAY_DURATION_MS: u64 = 1500;
pub const OSD_POSITION_Y_RATIO: f32 = 0.85;

pub const OSD_ROUNDING: f32 = 12.0;
pub const OSD_BORDER_WIDTH: f32 = 2.0;
pub const OSD_OUTER_MARGIN: f32 = 10.0;
pub const OSD_INNER_MARGIN: f32 = 10.0;

pub const SLIDER_BAR_WIDTH: f32 = 160.0;
pub const SLIDER_BAR_HEIGHT: f32 = 8.0;
pub const SLIDER_BAR_SPACING: f32 = 2.0;

pub const ICON_MAX_SIZE: egui::Vec2 = egui::vec2(80.0, 80.0);
pub const TEXT_FONT_SIZE: f32 = 27.0;

// ── OSD State ───────────────────────────────────────────────────────────────

/// Tracks the current visibility phase of the OSD overlay.
#[derive(PartialEq, Debug)]
pub enum OsdState {
    Hidden,
    FadingIn,
    Active,
    FadingOut,
}

// ── OSD Colors ──────────────────────────────────────────────────────────────

/// Computes OSD colors with the given alpha for fade animations.
pub struct OsdColors {
    pub background: egui::Color32,
    pub accent: egui::Color32,
    pub text: egui::Color32,
}

impl OsdColors {
    pub fn with_alpha(alpha: f32) -> Self {
        Self {
            background: egui::Color32::from_rgba_premultiplied(
                (30. * alpha) as u8,
                (30. * alpha) as u8,
                (30. * alpha) as u8,
                (230. * alpha) as u8,
            ),
            accent: egui::Color32::from_rgba_premultiplied(
                (255. * alpha) as u8,
                (215. * alpha) as u8,
                (0. * alpha) as u8,
                (230. * alpha) as u8,
            ),
            text: egui::Color32::from_rgba_premultiplied(
                (255. * alpha) as u8,
                (255. * alpha) as u8,
                (255. * alpha) as u8,
                (230. * alpha) as u8,
            ),
        }
    }
}

// ── Window Options ──────────────────────────────────────────────────────────

/// Returns the `NativeOptions` for the OSD overlay window.
pub fn native_options() -> NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_inner_size([OSD_WINDOW_SIZE.x, OSD_WINDOW_SIZE.y])
            .with_taskbar(false),
        ..Default::default()
    }
}
