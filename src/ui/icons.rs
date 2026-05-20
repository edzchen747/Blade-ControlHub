//! Centralized icon registry: all SVG assets, off-state overlays, and icon
//! lookup APIs for the OSD overlay and system tray.
//!
//! Change an icon asset once, propagate everywhere it's consumed.

use std::borrow::Cow;
use std::sync::LazyLock;

// ── OSD Icon Identifiers ────────────────────────────────────────────────────

/// Identifies which icon to display on the OSD overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsdIconId {
    RazerControlHub,
    Brightness,
    KeyboardBrightness,
    MicMute(bool),
    Trackpad(bool),
    RGBEffect,
    UnderGlow(bool),
    RefreshRate,
    BatteryLimit(bool),
    FunctionKey,
}

// ── Strike-through Overlay ──────────────────────────────────────────────────

/// The red diagonal strike-through line appended to "off" state icons.
const STRIKE_THROUGH_SVG: &str =
    r##"<path d="M2 22L22 2" stroke="#FF4444" stroke-width="2" stroke-linecap="round" />"##;

/// Overlays a red strike-through line onto an existing SVG icon by inserting
/// it just before the closing `</svg>` tag.
fn with_strikethrough(base_svg: &[u8]) -> Vec<u8> {
    let base = std::str::from_utf8(base_svg).expect("SVG asset must be valid UTF-8");
    base.replacen("</svg>", &format!("{STRIKE_THROUGH_SVG}\n</svg>"), 1)
        .into_bytes()
}

// Lazily-generated "off" state icons — computed once, cached forever.
static MIC_OFF_SVG: LazyLock<Vec<u8>> =
    LazyLock::new(|| with_strikethrough(include_bytes!("../../assets/mic.svg")));
static TRACKPAD_OFF_SVG: LazyLock<Vec<u8>> =
    LazyLock::new(|| with_strikethrough(include_bytes!("../../assets/trackpad.svg")));
static UNDERGLOW_OFF_SVG: LazyLock<Vec<u8>> =
    LazyLock::new(|| with_strikethrough(include_bytes!("../../assets/underglow.svg")));
static BATTERY_LIMIT_OFF: LazyLock<Vec<u8>> =
    LazyLock::new(|| with_strikethrough(include_bytes!("../../assets/battery_limit.svg")));

impl OsdIconId {
    /// Returns the `(uri, bytes)` pair for embedding the icon in the OSD.
    ///
    /// For "off" states the icon is generated dynamically by overlaying a red
    /// strike-through on the base icon — no separate `_off.svg` asset needed.
    pub fn icon_data(&self) -> (&'static str, Cow<'static, [u8]>) {
        match self {
            Self::RazerControlHub => (
                "bytes://icon.svg",
                Cow::Borrowed(include_bytes!("../../assets/icon.svg")),
            ),
            Self::Brightness => (
                "bytes://brightness.svg",
                Cow::Borrowed(include_bytes!("../../assets/brightness.svg")),
            ),
            Self::KeyboardBrightness => (
                "bytes://keyboard.svg",
                Cow::Borrowed(include_bytes!("../../assets/keyboard.svg")),
            ),
            Self::MicMute(false) => (
                "bytes://mic.svg",
                Cow::Borrowed(include_bytes!("../../assets/mic.svg")),
            ),
            Self::MicMute(true) => ("bytes://mic_off.svg", Cow::Borrowed(&MIC_OFF_SVG)),
            Self::Trackpad(true) => (
                "bytes://trackpad.svg",
                Cow::Borrowed(include_bytes!("../../assets/trackpad.svg")),
            ),
            Self::Trackpad(false) => ("bytes://trackpad_off.svg", Cow::Borrowed(&TRACKPAD_OFF_SVG)),
            Self::RGBEffect => (
                "bytes://rgb_effect.svg",
                Cow::Borrowed(include_bytes!("../../assets/rgb_effect.svg")),
            ),
            Self::UnderGlow(true) => (
                "bytes://underglow.svg",
                Cow::Borrowed(include_bytes!("../../assets/underglow.svg")),
            ),
            Self::UnderGlow(false) => (
                "bytes://underglow_off.svg",
                Cow::Borrowed(&UNDERGLOW_OFF_SVG),
            ),
            Self::RefreshRate => (
                "bytes://refresh.svg",
                Cow::Borrowed(include_bytes!("../../assets/refresh.svg")),
            ),
            Self::BatteryLimit(true) => (
                "bytes://battery_limit.svg",
                Cow::Borrowed(include_bytes!("../../assets/battery_limit.svg")),
            ),
            Self::BatteryLimit(false) => (
                "bytes://battery_limit_off.svg",
                Cow::Borrowed(&BATTERY_LIMIT_OFF),
            ),
            Self::FunctionKey => (
                "bytes://function_key.svg",
                Cow::Borrowed(include_bytes!("../../assets/function_key.svg")),
            ),
        }
    }
}
