use crate::{
    razer::enums::{BatteryLimit, LidLogoMode, PerfMode, RGBEffect},
    runtime::debug_mode,
    ui::osd_controller::OsdParams,
};

pub use crate::ui::icons::OsdIcon;

#[derive(PartialEq, Clone, serde::Serialize)]
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
    TogglePrimaryMultimediaKeys(bool),
    CloseGPUApps(bool),
    /// A custom key binding that has no overlay of its own: the label names
    /// what the action does, never the user's label for the row.
    CustomAction(String),
    /// A custom control built on the Command Lab page, and the side it was just
    /// switched to.
    CustomControl(String, bool),
    /// A key replayed a Command Lab capture. A capture has no state to show,
    /// so the card only names it.
    ReplayCapture(String),
}

/// How much of a custom control's name the overlay card can hold beside its
/// state. The name is the user's, so it is trimmed rather than left to run off
/// the edge of the card.
const CUSTOM_CONTROL_NAME_LIMIT: usize = 18;

/// A capture's name has the card to itself, so it can run to what a custom
/// control's name and state take up together.
const CAPTURE_NAME_LIMIT: usize = 24;

impl OsdEvent {
    pub fn as_params(&self) -> Option<OsdParams> {
        match self {
            OsdEvent::Startup => Some(OsdParams {
                label: if debug_mode::is_enabled() {
                    "Debug"
                } else {
                    "ControlHub"
                }
                .to_string(),
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
            OsdEvent::TogglePrimaryMultimediaKeys(is_multimedia) => {
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
            OsdEvent::CustomAction(label) => Some(OsdParams {
                label: label.clone(),
                icon: Some(OsdIcon::RazerControlHub),
                total_steps: 0,
                active_steps: 0,
            }),
            // A custom control is the user's own, so the card has to name it as
            // well as state it: the icon is shared by every one of them.
            OsdEvent::CustomControl(name, enabled) => Some(OsdParams {
                label: format!(
                    "{} · {}",
                    truncate(name, CUSTOM_CONTROL_NAME_LIMIT),
                    if *enabled { "On" } else { "Off" }
                ),
                icon: Some(OsdIcon::CommandLab),
                total_steps: 1,
                active_steps: *enabled as usize,
            }),
            // The same icon as a custom control, so everything a key does
            // through Command Lab reads as coming from one place.
            OsdEvent::ReplayCapture(name) => Some(OsdParams {
                label: truncate(name, CAPTURE_NAME_LIMIT),
                icon: Some(OsdIcon::CommandLab),
                total_steps: 0,
                active_steps: 0,
            }),
        }
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    text.chars()
        .take(limit.saturating_sub(1))
        .collect::<String>()
        + "…"
}

#[derive(PartialEq, Clone, serde::Serialize)]
pub enum AppEvent {
    OsdEvent(OsdEvent),
    OpenSettings,
    ToggleSettings,
    Restart(i32),
    Shutdown,
}

impl From<OsdEvent> for AppEvent {
    fn from(event: OsdEvent) -> Self {
        AppEvent::OsdEvent(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_event_label_reflects_debug_mode() {
        let params = OsdEvent::Startup.as_params().expect("startup has OSD");

        assert_eq!(
            params.label,
            if debug_mode::is_enabled() {
                "Debug"
            } else {
                "ControlHub"
            }
        );
        assert_eq!(params.icon, Some(OsdIcon::RazerControlHub));
        assert_eq!(params.total_steps, 0);
        assert_eq!(params.active_steps, 0);
    }

    #[test]
    fn enable_osd_event_has_no_overlay_params() {
        assert!(OsdEvent::EnableOSD(false).as_params().is_none());
        assert!(OsdEvent::EnableOSD(true).as_params().is_none());
    }

    #[test]
    fn brightness_event_maps_to_ten_step_level() {
        let params = OsdEvent::ScreenBrightness(70)
            .as_params()
            .expect("brightness has OSD");

        assert_eq!(params.label, "");
        assert_eq!(params.icon, Some(OsdIcon::Brightness));
        assert_eq!(params.total_steps, 10);
        assert_eq!(params.active_steps, 7);
    }

    #[test]
    fn mute_and_trackpad_events_map_boolean_state_to_single_step_level() {
        let muted = OsdEvent::MicMute(true).as_params().expect("mute has OSD");
        let unmuted = OsdEvent::MicMute(false).as_params().expect("mute has OSD");
        let trackpad_off = OsdEvent::Trackpad(false)
            .as_params()
            .expect("trackpad has OSD");

        assert_eq!(muted.icon, Some(OsdIcon::MicMute(true)));
        assert_eq!(muted.active_steps, 0);
        assert_eq!(unmuted.icon, Some(OsdIcon::MicMute(false)));
        assert_eq!(unmuted.active_steps, 1);
        assert_eq!(trackpad_off.icon, Some(OsdIcon::Trackpad(false)));
        assert_eq!(trackpad_off.total_steps, 1);
        assert_eq!(trackpad_off.active_steps, 0);
    }

    #[test]
    fn a_custom_control_names_itself_and_states_the_side_it_landed_on() {
        let on = OsdEvent::CustomControl("Snap Tap".to_owned(), true)
            .as_params()
            .expect("a custom control has an OSD");
        let off = OsdEvent::CustomControl("Snap Tap".to_owned(), false)
            .as_params()
            .expect("a custom control has an OSD");

        assert_eq!(on.label, "Snap Tap · On");
        assert_eq!(on.icon, Some(OsdIcon::CommandLab));
        assert_eq!(on.total_steps, 1);
        assert_eq!(on.active_steps, 1);
        assert_eq!(off.label, "Snap Tap · Off");
        assert_eq!(off.active_steps, 0);
    }

    #[test]
    fn a_long_custom_control_name_is_trimmed_to_fit_the_card() {
        let params = OsdEvent::CustomControl("Performance boost profile".to_owned(), true)
            .as_params()
            .expect("a custom control has an OSD");

        assert_eq!(params.label, "Performance boost… · On");
    }

    /// The reported bug: a key bound to a capture replayed it with nothing on
    /// screen, unlike every other action a key can run.
    #[test]
    fn a_replayed_capture_names_itself_on_the_command_lab_card() {
        let params = OsdEvent::ReplayCapture("snap tap on".to_owned())
            .as_params()
            .expect("a replayed capture has an OSD");

        assert_eq!(params.label, "snap tap on");
        assert_eq!(params.icon, Some(OsdIcon::CommandLab));
        assert_eq!(params.total_steps, 0, "a capture has no state to show");
    }

    #[test]
    fn a_long_capture_name_is_trimmed_to_fit_the_card() {
        let params = OsdEvent::ReplayCapture("a capture name that keeps on going".to_owned())
            .as_params()
            .expect("a replayed capture has an OSD");

        assert_eq!(params.label, "a capture name that kee…");
        assert_eq!(params.label.chars().count(), CAPTURE_NAME_LIMIT);
    }

    #[test]
    fn close_gpu_apps_events_use_progress_labels() {
        let started = OsdEvent::CloseGPUApps(false)
            .as_params()
            .expect("close GPU app status has OSD");
        let finished = OsdEvent::CloseGPUApps(true)
            .as_params()
            .expect("close GPU app status has OSD");

        assert_eq!(started.label, "Closing apps...");
        assert_eq!(finished.label, "Done");
        assert_eq!(started.icon, Some(OsdIcon::GPU));
        assert_eq!(finished.icon, Some(OsdIcon::GPU));
    }
}
