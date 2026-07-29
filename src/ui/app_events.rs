use serde::Serialize;

use crate::{
    razer::enums::{BatteryLimit, LidLogoMode, PerfMode, RGBEffect},
    ui::osd_controller::OsdParams,
};

// Re-export OsdIcon for convenience (canonical definition lives in icons.rs)
pub use crate::ui::icons::OsdIcon;

// ── Application Events ──────────────────────────────────────────────────────

/// High-level events that drive the application UI and system actions.
#[derive(PartialEq, Clone, Copy, serde::Serialize)]
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
    CloseGPUApps(bool),
}

impl OsdEvent {
    pub fn as_params(&self) -> Option<OsdParams> {
        match self {
            OsdEvent::Startup => Some(OsdParams {
                label: "Razer\nControlHub".to_string(),
                icon: Some(OsdIcon::RazerControlHub),
                total_steps: 0,
                active_steps: 0,
            }),
            OsdEvent::EnableOSD(_) => None,
            OsdEvent::ScreenBrightness(lvl) => Some(OsdParams {
                label: String::new(),
                icon: Some(OsdIcon::Brightness),
                total_steps: 10,
                active_steps: (lvl / 10) as usize,
            }),
            OsdEvent::KeyboardBrightness(lvl) => Some(OsdParams {
                label: String::new(),
                icon: Some(OsdIcon::KeyboardBrightness),
                total_steps: 5,
                active_steps: (lvl / 51) as usize,
            }),
            OsdEvent::PerfMode(mode) => Some(OsdParams {
                label: mode.to_string(),
                icon: None,
                total_steps: 0,
                active_steps: 0,
            }),
            OsdEvent::MicMute(muted) => Some(OsdParams {
                label: String::new(),
                icon: Some(OsdIcon::MicMute(*muted)),
                total_steps: 1,
                active_steps: (!*muted) as usize,
            }),
            OsdEvent::Trackpad(state) => Some(OsdParams {
                label: String::new(),
                icon: Some(OsdIcon::Trackpad(*state)),
                total_steps: 1,
                active_steps: *state as usize,
            }),
            OsdEvent::RGBEffect(effect) => Some(OsdParams {
                label: effect.to_string(),
                icon: Some(OsdIcon::RGBEffect),
                total_steps: 0,
                active_steps: 0,
            }),
            OsdEvent::UnderGlow(lvl) => Some(OsdParams {
                label: String::new(),
                icon: Some(OsdIcon::UnderGlow(*lvl > 0)),
                total_steps: 1,
                active_steps: (*lvl / 255) as usize,
            }),
            OsdEvent::RefreshRate(current, level, total) => Some(OsdParams {
                label: current.to_string(),
                icon: Some(OsdIcon::RefreshRate),
                total_steps: *total as usize,
                active_steps: *level as usize,
            }),
            OsdEvent::LidLogo(current) => Some(OsdParams {
                label: current.to_string(),
                icon: Some(OsdIcon::RefreshRate),
                total_steps: 2,
                active_steps: *current as usize,
            }),
            OsdEvent::BatteryLimit(current, level, total) => Some(OsdParams {
                label: BatteryLimit::from(*current).to_string(),
                icon: Some(OsdIcon::BatteryLimit(
                    BatteryLimit::from(*current) != BatteryLimit::Off,
                )),
                total_steps: *total as usize,
                active_steps: *level as usize,
            }),
            OsdEvent::ToggleDefaultMultimediaKeys(is_multimedia) => {
                let label = if *is_multimedia {
                    "Multimedia".to_string()
                } else {
                    "Function".to_string()
                };
                Some(OsdParams {
                    label,
                    icon: Some(OsdIcon::FunctionKey),
                    total_steps: 1,
                    active_steps: *is_multimedia as usize,
                })
            }
            OsdEvent::CloseGPUApps(finished) => {
                let label = if *finished {
                    "Done".to_string()
                } else {
                    "Closing apps...".to_string()
                };
                Some(OsdParams {
                    label,
                    icon: Some(OsdIcon::GPU),
                    total_steps: 0,
                    active_steps: 0,
                })
            }
        }
    }
}

#[derive(PartialEq, Clone, Copy, serde::Serialize)]
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
