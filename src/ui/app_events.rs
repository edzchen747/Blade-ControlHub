use std::borrow::Cow;
use std::sync::LazyLock;

use crate::{
    razer::enums::{BatteryLimit, LidLogoMode, PerfMode, RGBEffect},
    ui::tray,
};
use OsdEvent::*;

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
#[derive(PartialEq)]
pub enum OsdEvent {
    Startup,
    EnableOSD(bool),
    ScreenBrightness(u8),
    KeyboardBrightness(u8),
    PerfMode(PerfMode),
    MicMute(bool),
    Trackpad(bool),
    RGBEffect(RGBEffect),
    UnderGlow(u8),
    LidLogo(LidLogoMode),
    RefreshRate(u32, u8, u8),
    BatteryLimit(u8, u8, u8),
    ToggleDefaultMultimediaKeys(bool),
}

#[derive(PartialEq)]
pub enum AppEvent {
    OsdEvent(OsdEvent),
    RazerKeyCode(u8),
    OpenSettings,
    ToggleSettings,
    Shutdown,
}

impl From<OsdEvent> for AppEvent {
    fn from(event: OsdEvent) -> Self {
        AppEvent::OsdEvent(event)
    }
}

// ── Event Processing ────────────────────────────────────────────────────────

/// Processes an `AppEvent` and returns `Some(OsdResponse)` when the OSD should
/// be triggered, or `None` for other actions.
pub fn process_osd_event(
    event: AppEvent,
    tray_icon: &mut tray_icon::TrayIcon,
) -> Option<OsdResponse> {
    match event {
        AppEvent::OsdEvent(Startup) => Some(OsdResponse {
            text: "Razer\nControlHub".to_string(),
            icon_id: Some(OsdIconId::RazerControlHub),
            total_levels: 0,
            current_level: 0,
        }),
        AppEvent::OsdEvent(EnableOSD(_enable)) => None,
        AppEvent::OsdEvent(ScreenBrightness(lvl)) => Some(OsdResponse {
            text: String::new(),
            icon_id: Some(OsdIconId::Brightness),
            total_levels: 10,
            current_level: lvl / 10,
        }),
        AppEvent::OsdEvent(KeyboardBrightness(lvl)) => Some(OsdResponse {
            text: String::new(),
            icon_id: Some(OsdIconId::KeyboardBrightness),
            total_levels: 5,
            current_level: lvl / 51,
        }),
        AppEvent::OsdEvent(PerfMode(mode)) => {
            tray::set_perf_mode_icon(tray_icon, mode);
            Some(OsdResponse {
                text: mode.to_string(),
                icon_id: None,
                total_levels: 0,
                current_level: 0,
            })
        }
        AppEvent::OsdEvent(MicMute(muted)) => Some(OsdResponse {
            text: String::new(),
            icon_id: Some(OsdIconId::MicMute(muted)),
            total_levels: 1,
            current_level: !muted as u8,
        }),
        AppEvent::OsdEvent(Trackpad(state)) => Some(OsdResponse {
            text: String::new(),
            icon_id: Some(OsdIconId::Trackpad(state)),
            total_levels: 1,
            current_level: state as u8,
        }),
        AppEvent::OsdEvent(RGBEffect(effect)) => Some(OsdResponse {
            text: effect.to_string(),
            icon_id: Some(OsdIconId::RGBEffect),
            total_levels: 0,
            current_level: 0,
        }),
        AppEvent::OsdEvent(UnderGlow(lvl)) => Some(OsdResponse {
            text: String::new(),
            icon_id: Some(OsdIconId::UnderGlow(lvl > 0)),
            total_levels: 1,
            current_level: lvl / 255,
        }),
        AppEvent::OsdEvent(RefreshRate(current, level, total)) => Some(OsdResponse {
            text: current.to_string(),
            icon_id: Some(OsdIconId::RefreshRate),
            total_levels: total,
            current_level: level,
        }),
        AppEvent::OsdEvent(LidLogo(current)) => Some(OsdResponse {
            text: current.to_string(),
            icon_id: Some(OsdIconId::RefreshRate),
            total_levels: 2,
            current_level: current as u8,
        }),
        AppEvent::OsdEvent(BatteryLimit(current, level, total)) => Some(OsdResponse {
            text: BatteryLimit::from(current).to_string(),
            icon_id: Some(OsdIconId::BatteryLimit(
                BatteryLimit::from(current) != BatteryLimit::Off,
            )),
            total_levels: total,
            current_level: level,
        }),
        AppEvent::OsdEvent(ToggleDefaultMultimediaKeys(is_multimedia)) => {
            let text = match is_multimedia {
                true => "Multimedia".to_string(),
                false => "Function".to_string(),
            };
            Some(OsdResponse {
                text: text,
                icon_id: Some(OsdIconId::FunctionKey),
                total_levels: 1,
                current_level: is_multimedia as u8,
            })
        }
        _ => None,
    }
}
