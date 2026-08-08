use crate::razer::enums::PerfMode;
use crate::ui::app_events::{AppEvent, OsdEvent};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SideEffect {
    ToggleSettings,
    OpenSettings,
    Restart(i32),
    Shutdown,
    EnableOsd(bool),
    PerfMode(PerfMode),
}

pub struct EventDispatcher;

impl EventDispatcher {
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
    }

    #[test]
    fn dispatch_ignores_osd_only_events() {
        assert_eq!(
            EventDispatcher::dispatch(AppEvent::OsdEvent(OsdEvent::ScreenBrightness(50))),
            None
        );
    }
}
