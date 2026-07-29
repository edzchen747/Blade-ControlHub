//! Event dispatcher: separates app events that require platform side effects
//! from events that only drive OSD rendering.
//!
//! OSD parameter generation lives on `OsdEvent`; this module only answers
//! whether an incoming event also needs an application-level action.

use crate::razer::enums::PerfMode;
use crate::ui::app_events::{AppEvent, OsdEvent};

// ── Side Effects ─────────────────────────────────────────────────────────────

/// Actions the application must perform in response to an event,
/// independent of OSD rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SideEffect {
    ToggleSettings,
    OpenSettings,
    Restart(i32),
    Shutdown,
    EnableOsd(bool),
    RazerKeyCode(u8),
    PerfMode(PerfMode),
}

// ── Event Dispatcher ─────────────────────────────────────────────────────────

/// Routes a single `AppEvent` into an optional `SideEffect`.
/// Callers process the side effect first, then optionally display OSD output.
pub struct EventDispatcher;

impl EventDispatcher {
    /// Dispatches an `AppEvent`, returning the optional side effect.
    pub fn dispatch(event: AppEvent) -> Option<SideEffect> {
        Self::extract_side_effect(&event)
    }

    fn extract_side_effect(event: &AppEvent) -> Option<SideEffect> {
        match event {
            AppEvent::ToggleSettings => Some(SideEffect::ToggleSettings),
            AppEvent::OpenSettings => Some(SideEffect::OpenSettings),
            AppEvent::Restart(code) => Some(SideEffect::Restart(*code)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_maps_settings_events_to_side_effects() {
        assert_eq!(
            EventDispatcher::dispatch(AppEvent::ToggleSettings),
            Some(SideEffect::ToggleSettings)
        );
        assert_eq!(
            EventDispatcher::dispatch(AppEvent::OpenSettings),
            Some(SideEffect::OpenSettings)
        );
    }

    #[test]
    fn dispatch_maps_shutdown_to_side_effect() {
        assert_eq!(
            EventDispatcher::dispatch(AppEvent::Shutdown),
            Some(SideEffect::Shutdown)
        );
    }

    #[test]
    fn dispatch_maps_restart_to_side_effect() {
        assert_eq!(
            EventDispatcher::dispatch(AppEvent::Restart(1)),
            Some(SideEffect::Restart(1))
        );
    }

    #[test]
    fn dispatch_maps_control_events_to_side_effects() {
        assert_eq!(
            EventDispatcher::dispatch(AppEvent::OsdEvent(OsdEvent::EnableOSD(false))),
            Some(SideEffect::EnableOsd(false))
        );
        assert_eq!(
            EventDispatcher::dispatch(AppEvent::OsdEvent(OsdEvent::PerfMode(
                PerfMode::Performance,
            ))),
            Some(SideEffect::PerfMode(PerfMode::Performance))
        );
        assert_eq!(
            EventDispatcher::dispatch(AppEvent::RazerKeyCode(0x42)),
            Some(SideEffect::RazerKeyCode(0x42))
        );
    }

    #[test]
    fn dispatch_ignores_osd_only_events() {
        assert_eq!(
            EventDispatcher::dispatch(AppEvent::OsdEvent(OsdEvent::ScreenBrightness(50))),
            None
        );
    }
}
