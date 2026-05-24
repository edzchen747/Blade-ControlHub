use crate::razer::enums::{LidLogoMode, PerfMode, RGBEffect};

// Re-export OsdIconId for convenience (canonical definition lives in icons.rs)
pub use crate::ui::icons::OsdIconId;

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
    CloseGPUApps(bool),
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
