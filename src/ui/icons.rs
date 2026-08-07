//! Centralized icon registry: all SVG assets, off-state overlays, and icon
//! lookup APIs for the OSD overlay and system tray.
//!
//! Change an icon asset once, propagate everywhere it's consumed.

use std::borrow::Cow;
use tracing::warn;

// ── OSD Icon Identifiers ────────────────────────────────────────────────────

/// Identifies which icon to display on the OSD overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsdIcon {
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
    GPU,
    CommandLab,
}

// ── Strike-through Overlay ──────────────────────────────────────────────────

/// The red diagonal strike-through line appended to "off" state icons.
const STRIKE_THROUGH_SVG: &str =
    r##"<path d="M2 22L22 2" stroke="#FF4444" stroke-width="2" stroke-linecap="round" />"##;

/// Overlays a red strike-through line onto an existing SVG icon by inserting
/// it just before the closing `</svg>` tag.
fn with_strikethrough(base_svg: &[u8]) -> Cow<'static, [u8]> {
    match std::str::from_utf8(base_svg) {
        Ok(base) => Cow::Owned(
            base.replacen("</svg>", &format!("{STRIKE_THROUGH_SVG}\n</svg>"), 1)
                .into_bytes(),
        ),
        Err(error) => {
            warn!(
                ?error,
                "OSD icon SVG asset was not valid UTF-8; using base icon"
            );
            Cow::Owned(base_svg.to_vec())
        }
    }
}

impl OsdIcon {
    /// Stable identity key for the control this icon represents. Boolean state
    /// variants (e.g. mic on/off) share a key so toggling the same control does
    /// not create a new OSD card.
    pub fn kind_key(&self) -> u8 {
        match self {
            Self::RazerControlHub => 0,
            Self::Brightness => 1,
            Self::KeyboardBrightness => 2,
            Self::MicMute(_) => 3,
            Self::Trackpad(_) => 4,
            Self::RGBEffect => 5,
            Self::UnderGlow(_) => 6,
            Self::RefreshRate => 7,
            Self::BatteryLimit(_) => 8,
            Self::FunctionKey => 9,
            Self::GPU => 10,
            Self::CommandLab => 11,
        }
    }

    /// Returns the `(uri, bytes)` pair for embedding the icon in the OSD.
    ///
    /// For "off" states the icon is generated dynamically by overlaying a red
    /// strike-through on the base icon — no separate `_off.svg` asset needed.
    pub fn as_bytes(&self) -> Cow<'static, [u8]> {
        match self {
            Self::RazerControlHub => Cow::Borrowed(include_bytes!("../../assets/icon.svg")),
            Self::Brightness => Cow::Borrowed(include_bytes!("../../assets/brightness.svg")),
            Self::KeyboardBrightness => Cow::Borrowed(include_bytes!("../../assets/keyboard.svg")),
            Self::MicMute(false) => Cow::Borrowed(include_bytes!("../../assets/mic.svg")),
            Self::MicMute(true) => with_strikethrough(include_bytes!("../../assets/mic.svg")),
            Self::Trackpad(true) => Cow::Borrowed(include_bytes!("../../assets/trackpad.svg")),
            Self::Trackpad(false) => {
                with_strikethrough(include_bytes!("../../assets/trackpad.svg"))
            }
            Self::RGBEffect => Cow::Borrowed(include_bytes!("../../assets/rgb_effect.svg")),
            Self::UnderGlow(true) => Cow::Borrowed(include_bytes!("../../assets/underglow.svg")),
            Self::UnderGlow(false) => {
                with_strikethrough(include_bytes!("../../assets/underglow.svg"))
            }
            Self::RefreshRate => Cow::Borrowed(include_bytes!("../../assets/refresh.svg")),
            Self::BatteryLimit(true) => {
                Cow::Borrowed(include_bytes!("../../assets/battery_limit.svg"))
            }
            Self::BatteryLimit(false) => {
                with_strikethrough(include_bytes!("../../assets/battery_limit.svg"))
            }
            Self::FunctionKey => Cow::Borrowed(include_bytes!("../../assets/function_key.svg")),
            Self::GPU => Cow::Borrowed(include_bytes!("../../assets/gpu.svg")),
            Self::CommandLab => Cow::Borrowed(include_bytes!("../../assets/command_lab.svg")),
        }
    }
}
