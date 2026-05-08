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

impl OsdIconId {
    /// Returns the `(uri, bytes)` pair for embedding the icon in the OSD.
    pub fn icon_data(&self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Brightness => (
                "bytes://brightness.svg",
                include_bytes!("../../assets/brightness.svg"),
            ),
            Self::KeyboardBrightness => (
                "bytes://keyboard.svg",
                include_bytes!("../../assets/keyboard.svg"),
            ),
            Self::MicMute(false) => ("bytes://mic.svg", include_bytes!("../../assets/mic.svg")),
            Self::MicMute(true) => (
                "bytes://mic_off.svg",
                include_bytes!("../../assets/mic_off.svg"),
            ),
            Self::Trackpad(true) => (
                "bytes://trackpad.svg",
                include_bytes!("../../assets/trackpad.svg"),
            ),
            Self::Trackpad(false) => (
                "bytes://trackpad_off.svg",
                include_bytes!("../../assets/trackpad_off.svg"),
            ),
            Self::RGBEffect => (
                "bytes://rgb_effect.svg",
                include_bytes!("../../assets/rgb_effect.svg"),
            ),
            Self::UnderGlow(true) => (
                "bytes://underglow.svg",
                include_bytes!("../../assets/underglow.svg"),
            ),
            Self::UnderGlow(false) => (
                "bytes://underglow_off.svg",
                include_bytes!("../../assets/underglow_off.svg"),
            ),
            Self::RefreshRate => (
                "bytes://refresh.svg",
                include_bytes!("../../assets/refresh.svg"),
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
