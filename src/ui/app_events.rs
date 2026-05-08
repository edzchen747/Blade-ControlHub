use std::borrow::Cow;
use std::sync::LazyLock;

use crate::{
    razer::{
        device_handle::device,
        enums::{PerfMode, RGBEffect},
    },
    ui::icon,
    utils::reload::restart_app,
    win::system::startup::Startup,
};

// ── OSD Icon Identifiers ────────────────────────────────────────────────────

/// Identifies which icon to display on the OSD overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsdIconId {
    Brightness,
    KeyboardBrightness,
    MicMute(bool),
    Trackpad(bool),
    RGBEffect,
    UnderGlow(bool),
    RefreshRate,
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

impl OsdIconId {
    /// Returns the `(uri, bytes)` pair for embedding the icon in the OSD.
    ///
    /// For "off" states the icon is generated dynamically by overlaying a red
    /// strike-through on the base icon — no separate `_off.svg` asset needed.
    pub fn icon_data(&self) -> (&'static str, Cow<'static, [u8]>) {
        match self {
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
        }
    }
}

// ── OSD Response ─────────────────────────────────────────────────────────────

/// Describes what the OSD should display after processing an event.
pub struct OsdResponse {
    pub text: String,
    pub icon_id: Option<OsdIconId>,
    pub total_levels: u8,
    pub current_level: u8,
}

// ── Application Events ──────────────────────────────────────────────────────

/// High-level events that drive the application UI and system actions.
pub enum AppEvent {
    ScreenBrightness(u8),
    KeyboardBrightness(u8),
    PerfMode(PerfMode),
    MicMute(bool),
    Trackpad(bool),
    RGBEffect(RGBEffect),
    UnderGlow(u8),
    RefreshRate(u32, u8, u8),
    Quit,
    Restart,
    StartupToggle(bool),
}

// ── Event Processing ────────────────────────────────────────────────────────

/// Processes an `AppEvent` and returns `Some(OsdResponse)` when the OSD should
/// be triggered, or `None` for silent actions.
pub fn process_event(event: AppEvent, tray_icon: &mut tray_icon::TrayIcon) -> Option<OsdResponse> {
    match event {
        AppEvent::ScreenBrightness(lvl) => Some(OsdResponse {
            text: String::new(),
            icon_id: Some(OsdIconId::Brightness),
            total_levels: 10,
            current_level: lvl / 10,
        }),
        AppEvent::KeyboardBrightness(lvl) => Some(OsdResponse {
            text: String::new(),
            icon_id: Some(OsdIconId::KeyboardBrightness),
            total_levels: 5,
            current_level: lvl / 51,
        }),
        AppEvent::PerfMode(mode) => {
            icon::set_perf_mode_icon(tray_icon, mode);
            Some(OsdResponse {
                text: mode.to_string(),
                icon_id: None,
                total_levels: 0,
                current_level: 0,
            })
        }
        AppEvent::MicMute(muted) => Some(OsdResponse {
            text: String::new(),
            icon_id: Some(OsdIconId::MicMute(muted)),
            total_levels: 1,
            current_level: !muted as u8,
        }),
        AppEvent::Trackpad(state) => Some(OsdResponse {
            text: String::new(),
            icon_id: Some(OsdIconId::Trackpad(state)),
            total_levels: 1,
            current_level: state as u8,
        }),
        AppEvent::RGBEffect(effect) => Some(OsdResponse {
            text: effect.to_string(),
            icon_id: Some(OsdIconId::RGBEffect),
            total_levels: 0,
            current_level: 0,
        }),
        AppEvent::UnderGlow(lvl) => Some(OsdResponse {
            text: String::new(),
            icon_id: Some(OsdIconId::UnderGlow(lvl > 0)),
            total_levels: 1,
            current_level: lvl / 255,
        }),
        AppEvent::RefreshRate(current, level, total) => Some(OsdResponse {
            text: current.to_string(),
            icon_id: Some(OsdIconId::RefreshRate),
            total_levels: total,
            current_level: level,
        }),
        AppEvent::Quit => {
            device().shutdown();
            std::process::exit(0);
        }
        AppEvent::Restart => restart_app(0),
        AppEvent::StartupToggle(enabled) => {
            if enabled && !Startup::is_registered() {
                Startup::register();
            } else if !enabled && Startup::is_registered() {
                Startup::unregister();
            }
            None
        }
    }
}
