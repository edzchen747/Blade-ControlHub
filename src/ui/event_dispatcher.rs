//! Event dispatcher: separates OSD response generation from side-effect handling.
//!
//! Provides a single `EventDispatcher` struct that routes `AppEvent`s into
//! two independent output paths:
//! 1. `OsdResponse` — what the OSD overlay should display
//! 2. `SideEffect` — actions the application must perform (settings, shutdown, etc.)

use crate::razer::enums::{BatteryLimit, PerfMode};
use crate::ui::app_events::{AppEvent, OsdEvent, OsdResponse};
use crate::ui::icons::OsdIconId;

// ── Side Effects ─────────────────────────────────────────────────────────────

/// Actions the application must perform in response to an event,
/// independent of OSD rendering.
#[derive(Debug)]
pub enum SideEffect {
    ToggleSettings,
    OpenSettings,
    Shutdown,
    EnableOsd(bool),
    RazerKeyCode(u8),
    PerfMode(PerfMode),
}

// ── Event Dispatcher ─────────────────────────────────────────────────────────

/// Routes a single `AppEvent` into its `OsdResponse` and `SideEffect` components.
/// Callers process the side effect first, then optionally display the OSD response.
pub struct EventDispatcher;

impl EventDispatcher {
    /// Dispatches an `AppEvent`, returning both the optional `OsdResponse` and
    /// the optional `SideEffect`. Order of operations: side effect first, OSD second.
    pub fn dispatch(event: AppEvent) -> (Option<OsdResponse>, Option<SideEffect>) {
        let osd = Self::generate_osd(&event);
        let side = Self::extract_side_effect(&event);
        (osd, side)
    }

    // ── OSD Response Generation ─────────────────────────────────────────

    fn generate_osd(event: &AppEvent) -> Option<OsdResponse> {
        match event {
            AppEvent::OsdEvent(OsdEvent::Startup) => Some(OsdResponse {
                text: "Razer\nControlHub".to_string(),
                icon_id: Some(OsdIconId::RazerControlHub),
                total_levels: 0,
                current_level: 0,
            }),
            AppEvent::OsdEvent(OsdEvent::EnableOSD(_)) => None,
            AppEvent::OsdEvent(OsdEvent::ScreenBrightness(lvl)) => Some(OsdResponse {
                text: String::new(),
                icon_id: Some(OsdIconId::Brightness),
                total_levels: 10,
                current_level: lvl / 10,
            }),
            AppEvent::OsdEvent(OsdEvent::KeyboardBrightness(lvl)) => Some(OsdResponse {
                text: String::new(),
                icon_id: Some(OsdIconId::KeyboardBrightness),
                total_levels: 5,
                current_level: lvl / 51,
            }),
            AppEvent::OsdEvent(OsdEvent::PerfMode(mode)) => Some(OsdResponse {
                text: mode.to_string(),
                icon_id: None,
                total_levels: 0,
                current_level: 0,
            }),
            AppEvent::OsdEvent(OsdEvent::MicMute(muted)) => Some(OsdResponse {
                text: String::new(),
                icon_id: Some(OsdIconId::MicMute(*muted)),
                total_levels: 1,
                current_level: !*muted as u8,
            }),
            AppEvent::OsdEvent(OsdEvent::Trackpad(state)) => Some(OsdResponse {
                text: String::new(),
                icon_id: Some(OsdIconId::Trackpad(*state)),
                total_levels: 1,
                current_level: *state as u8,
            }),
            AppEvent::OsdEvent(OsdEvent::RGBEffect(effect)) => Some(OsdResponse {
                text: effect.to_string(),
                icon_id: Some(OsdIconId::RGBEffect),
                total_levels: 0,
                current_level: 0,
            }),
            AppEvent::OsdEvent(OsdEvent::UnderGlow(lvl)) => Some(OsdResponse {
                text: String::new(),
                icon_id: Some(OsdIconId::UnderGlow(*lvl > 0)),
                total_levels: 1,
                current_level: *lvl / 255,
            }),
            AppEvent::OsdEvent(OsdEvent::RefreshRate(current, level, total)) => Some(OsdResponse {
                text: current.to_string(),
                icon_id: Some(OsdIconId::RefreshRate),
                total_levels: *total,
                current_level: *level,
            }),
            AppEvent::OsdEvent(OsdEvent::LidLogo(current)) => Some(OsdResponse {
                text: current.to_string(),
                icon_id: Some(OsdIconId::RefreshRate),
                total_levels: 2,
                current_level: *current as u8,
            }),
            AppEvent::OsdEvent(OsdEvent::BatteryLimit(current, level, total)) => {
                Some(OsdResponse {
                    text: BatteryLimit::from(*current).to_string(),
                    icon_id: Some(OsdIconId::BatteryLimit(
                        BatteryLimit::from(*current) != BatteryLimit::Off,
                    )),
                    total_levels: *total,
                    current_level: *level,
                })
            }
            AppEvent::OsdEvent(OsdEvent::ToggleDefaultMultimediaKeys(is_multimedia)) => {
                let text = if *is_multimedia {
                    "Multimedia".to_string()
                } else {
                    "Function".to_string()
                };
                Some(OsdResponse {
                    text,
                    icon_id: Some(OsdIconId::FunctionKey),
                    total_levels: 1,
                    current_level: *is_multimedia as u8,
                })
            }
            AppEvent::OsdEvent(OsdEvent::CloseGPUApps(finished)) => {
                let text = if *finished {
                    "Done".to_string()
                } else {
                    "Closing apps...".to_string()
                };
                Some(OsdResponse {
                    text,
                    icon_id: Some(OsdIconId::GPU),
                    total_levels: 0,
                    current_level: 0,
                })
            }
            AppEvent::RazerKeyCode(_)
            | AppEvent::OpenSettings
            | AppEvent::ToggleSettings
            | AppEvent::Shutdown => None,
        }
    }

    // ── Side Effect Extraction ──────────────────────────────────────────

    fn extract_side_effect(event: &AppEvent) -> Option<SideEffect> {
        match event {
            AppEvent::ToggleSettings => Some(SideEffect::ToggleSettings),
            AppEvent::OpenSettings => Some(SideEffect::OpenSettings),
            AppEvent::Shutdown => Some(SideEffect::Shutdown),
            AppEvent::OsdEvent(OsdEvent::EnableOSD(enable)) => Some(SideEffect::EnableOsd(*enable)),
            AppEvent::OsdEvent(OsdEvent::PerfMode(mode)) => Some(SideEffect::PerfMode(*mode)),
            AppEvent::RazerKeyCode(key_code) => Some(SideEffect::RazerKeyCode(*key_code)),
            _ => None,
        }
    }
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self
    }
}
