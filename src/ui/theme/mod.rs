//! Colours and sizing shared by the runtime surfaces (OSD, tray, window icon).
//!
//! Deliberately free of any UI-toolkit types: the OSD paints through `resvg`
//! into a layered Win32 window and the settings window is HTML, so a common
//! colour type here would only be a conversion tax on both.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::config::ThemeColor;
use crate::razer::enums::PerfMode;

pub const DEFAULT_ICON_COLOR: &str = "#95A5A6";

pub const SETTINGS_LOADING_ICON_COLOR: ThemeColor = ThemeColor::new(0x95, 0xa5, 0xa6);

pub const APP_TOOLTIP: &str = "Blade ControlHub";

pub const SETTINGS_WINDOW_TITLE: &str = "Blade ControlHub";

/// Square edge of the rendered tray icon, in pixels.
pub const TRAY_ICON_SIZE: u32 = 64;

/// The tray glyph is drawn slightly larger than its viewBox so it reads at
/// 16 px after Windows downscales it.
pub const TRAY_ICON_SCALE_FACTOR: f32 = 1.2;

/// Square edge of the rendered settings-window icon, in pixels.
pub const SETTINGS_ICON_SIZE: u32 = 64;

/// Fraction of screen height kept between the settings window and the bottom
/// right corner of the primary monitor.
pub const SETTINGS_PADDING_RATIO: f32 = 0.1;

pub const OSD_DISPLAY_DURATION_MS: u64 = 1500;

pub const FADE_OUT_SPEED: f32 = 2.0;

pub const TOTAL_ANIM_TIME_MS: f32 = OSD_DISPLAY_DURATION_MS as f32 + 800.0 / FADE_OUT_SPEED;

static RUNTIME_THEME_COLOR: AtomicU32 =
    AtomicU32::new(theme_color_to_u32(ThemeColor::new(0xff, 0xd7, 0x00)));

pub fn set_runtime_theme_color(color: ThemeColor) {
    RUNTIME_THEME_COLOR.store(theme_color_to_u32(color), Ordering::SeqCst);
}

pub fn runtime_theme_color() -> ThemeColor {
    theme_color_from_u32(RUNTIME_THEME_COLOR.load(Ordering::SeqCst))
}

/// Black or white, whichever stays readable on top of `color`.
pub fn theme_text_color(color: ThemeColor) -> ThemeColor {
    let luminance = 0.2126 * color.r as f32 + 0.7152 * color.g as f32 + 0.0722 * color.b as f32;
    if luminance > 145.0 {
        ThemeColor::new(0, 0, 0)
    } else {
        ThemeColor::new(0xff, 0xff, 0xff)
    }
}

pub fn perf_mode_hex_color(mode: PerfMode) -> &'static str {
    perf_mode_color_components(mode).0
}

pub fn perf_mode_rgb(mode: PerfMode) -> ThemeColor {
    let (_, r, g, b) = perf_mode_color_components(mode);
    ThemeColor::new(r, g, b)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_theme_color_round_trips_through_its_packed_form() {
        set_runtime_theme_color(ThemeColor::new(0x12, 0x34, 0x56));

        assert_eq!(runtime_theme_color(), ThemeColor::new(0x12, 0x34, 0x56));
    }

    #[test]
    fn theme_text_color_flips_at_the_luminance_threshold() {
        assert_eq!(
            theme_text_color(ThemeColor::new(0xff, 0xd7, 0x00)),
            ThemeColor::new(0, 0, 0)
        );
        assert_eq!(
            theme_text_color(ThemeColor::new(0x20, 0x20, 0x40)),
            ThemeColor::new(0xff, 0xff, 0xff)
        );
    }

    #[test]
    fn perf_mode_hex_and_rgb_describe_the_same_colour() {
        assert_eq!(perf_mode_hex_color(PerfMode::Turbo), "#D50000");
        assert_eq!(perf_mode_rgb(PerfMode::Turbo), ThemeColor::new(0xd5, 0, 0));
    }
}
