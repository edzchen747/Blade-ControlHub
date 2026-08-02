//! Centralized UI theming: color palettes, typography, and styling helpers.
//!
//! Layout dimensions and spacing tokens have been moved to `crate::ui::layout`.
//! This module now focuses exclusively on colors, fonts, and visual effects.

use eframe::egui;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::config::ThemeColor;
use crate::razer::enums::PerfMode;

// ── Re-exports for backward compatibility ─────────────────────────────────────

/// Layout constants (window sizes, margins, spacing, animation timing).
/// Import from `crate::ui::layout` for new code.
pub use super::layout::*;

// ── Tray Theme ───────────────────────────────────────────────────────────────

/// Default icon color (hex string for SVG replacement).
pub const DEFAULT_ICON_COLOR: &str = "#95A5A6";

/// Neutral icon/accent color used before the runtime settings state is available.
pub const SETTINGS_LOADING_ICON_COLOR: ThemeColor = ThemeColor::new(0x95, 0xa5, 0xa6);

/// Tooltip text displayed when hovering the tray icon.
pub const APP_TOOLTIP: &str = "Blade ControlHub";

/// Title shown on the settings window frame.
pub const SETTINGS_WINDOW_TITLE: &str = "Blade ControlHub";

static RUNTIME_THEME_COLOR: AtomicU32 =
    AtomicU32::new(theme_color_to_u32(ThemeColor::new(0xff, 0xd7, 0x00)));

// ── OSD Colors ───────────────────────────────────────────────────────────────

/// Raw RGBA components for the OSD background at full opacity.
/// Computed as `(30, 30, 30, 230)` before alpha multiplication.
pub const OSD_BACKGROUND_R: f32 = 30.0;
pub const OSD_BACKGROUND_G: f32 = 30.0;
pub const OSD_BACKGROUND_B: f32 = 30.0;
pub const OSD_BACKGROUND_A: f32 = 230.0;

/// Raw RGBA components for the OSD accent (gold) color.
/// Computed as `(255, 215, 0, 230)` before alpha multiplication.
pub const OSD_ACCENT_R: f32 = 255.0;
pub const OSD_ACCENT_G: f32 = 215.0;
pub const OSD_ACCENT_B: f32 = 0.0;
pub const OSD_ACCENT_A: f32 = 230.0;

/// Raw RGBA components for the OSD text (white).
/// Computed as `(255, 255, 255, 230)` before alpha multiplication.
pub const OSD_TEXT_R: f32 = 255.0;
pub const OSD_TEXT_G: f32 = 255.0;
pub const OSD_TEXT_B: f32 = 255.0;
pub const OSD_TEXT_A: f32 = 230.0;

// ── Theme Helpers ────────────────────────────────────────────────────────────

/// Computes an OSD color with the given alpha factor applied for fade animations.
#[derive(Clone, Copy, Debug)]
pub struct OsdColors {
    /// Background fill color with alpha applied.
    pub background: egui::Color32,
    /// Accent (gold) color with alpha applied.
    pub accent: egui::Color32,
    /// Text (white) color with alpha applied.
    pub text: egui::Color32,
}

impl OsdColors {
    /// Creates a themed color set with the specified alpha (0.0 = invisible, 1.0 = full).
    #[inline]
    pub fn with_alpha(alpha: f32) -> Self {
        Self {
            background: egui::Color32::from_rgba_premultiplied(
                (Self::clamp(OSD_BACKGROUND_R * alpha)) as u8,
                (Self::clamp(OSD_BACKGROUND_G * alpha)) as u8,
                (Self::clamp(OSD_BACKGROUND_B * alpha)) as u8,
                (Self::clamp(OSD_BACKGROUND_A * alpha)) as u8,
            ),
            accent: egui::Color32::from_rgba_premultiplied(
                (Self::clamp(OSD_ACCENT_R * alpha)) as u8,
                (Self::clamp(OSD_ACCENT_G * alpha)) as u8,
                (Self::clamp(OSD_ACCENT_B * alpha)) as u8,
                (Self::clamp(OSD_ACCENT_A * alpha)) as u8,
            ),
            text: egui::Color32::from_rgba_premultiplied(
                (Self::clamp(OSD_TEXT_R * alpha)) as u8,
                (Self::clamp(OSD_TEXT_G * alpha)) as u8,
                (Self::clamp(OSD_TEXT_B * alpha)) as u8,
                (Self::clamp(OSD_TEXT_A * alpha)) as u8,
            ),
        }
    }

    #[inline]
    fn clamp(v: f32) -> f32 {
        v.clamp(0.0, 255.0)
    }
}

pub fn set_runtime_theme_color(color: ThemeColor) {
    RUNTIME_THEME_COLOR.store(theme_color_to_u32(color), Ordering::SeqCst);
}

pub fn runtime_theme_color() -> ThemeColor {
    theme_color_from_u32(RUNTIME_THEME_COLOR.load(Ordering::SeqCst))
}

pub fn theme_color32(color: ThemeColor) -> egui::Color32 {
    egui::Color32::from_rgb(color.r, color.g, color.b)
}

pub fn scaled_theme_color32(color: ThemeColor, scale: f32) -> egui::Color32 {
    let scale = scale.clamp(0.0, 1.0);
    egui::Color32::from_rgb(
        (color.r as f32 * scale).round() as u8,
        (color.g as f32 * scale).round() as u8,
        (color.b as f32 * scale).round() as u8,
    )
}

pub fn theme_text_color(color: ThemeColor) -> egui::Color32 {
    let luminance = 0.2126 * color.r as f32 + 0.7152 * color.g as f32 + 0.0722 * color.b as f32;
    if luminance > 145.0 {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    }
}

pub fn perf_mode_hex_color(mode: PerfMode) -> &'static str {
    perf_mode_color_components(mode).0
}

pub fn perf_mode_color32(mode: PerfMode) -> egui::Color32 {
    let (_, r, g, b) = perf_mode_color_components(mode);
    egui::Color32::from_rgb(r, g, b)
}

fn perf_mode_color_components(mode: PerfMode) -> (&'static str, u8, u8, u8) {
    match mode {
        PerfMode::BatterySaver => ("#9BF542", 0x9b, 0xf5, 0x42),
        PerfMode::Silent => ("#00C853", 0x00, 0xc8, 0x53),
        PerfMode::Quiet => ("#00E5FF", 0x00, 0xe5, 0xff),
        PerfMode::Balanced => ("#FFD600", 0xff, 0xd6, 0x00),
        PerfMode::Performance => ("#FF5D00", 0xff, 0x5d, 0x00),
        PerfMode::Turbo => ("#D50000", 0xd5, 0x00, 0x00),
        PerfMode::Custom => ("#A200FF", 0xa2, 0x00, 0xff),
        PerfMode::Unsupported => ("#FF00FF", 0xff, 0x00, 0xff),
        PerfMode::Unknown => (DEFAULT_ICON_COLOR, 0x95, 0xa5, 0xa6),
    }
}

const fn theme_color_to_u32(color: ThemeColor) -> u32 {
    ((color.r as u32) << 16) | ((color.g as u32) << 8) | color.b as u32
}

fn theme_color_from_u32(value: u32) -> ThemeColor {
    ThemeColor::new(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    )
}
