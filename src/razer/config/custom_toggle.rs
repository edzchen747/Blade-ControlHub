use crate::razer::config::PowerProfile;
use serde::{Deserialize, Serialize};

/// A two-state control the user builds on the Command Lab page out of two
/// saved captures: one is replayed to switch it on, the other to switch it off.
///
/// The captures are named rather than copied, so re-recording one under the
/// same name changes what the toggle does without touching the toggle.
///
/// The pair of captures is one definition, but which side the control is on is
/// remembered per power profile, the way a fan speed or a performance mode is:
/// a control worth having on while plugged in is often the one to drop on
/// battery. Switching profiles re-applies the side that profile was left on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomToggle {
    pub name: String,
    /// A capture name, or empty while the user has not chosen one yet.
    pub on_capture: String,
    pub off_capture: String,
    /// The side this control is on while plugged in.
    pub ac_enabled: bool,
    /// The side this control is on while on battery.
    pub battery_enabled: bool,
    /// The one side controls had before they were remembered per profile. It is
    /// read from an older config and folded into both, then never written
    /// again — see [`super::AppConfig::refresh_cycle_items`].
    #[serde(rename = "enabled", skip_serializing)]
    pub(super) legacy_enabled: bool,
}

impl CustomToggle {
    /// A control with both profiles switched off, which is how one starts.
    ///
    /// A constructor rather than a struct literal because the pre-split side is
    /// a private migration detail that no caller should be naming.
    pub fn new(
        name: impl Into<String>,
        on_capture: impl Into<String>,
        off_capture: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            on_capture: on_capture.into(),
            off_capture: off_capture.into(),
            ..Self::default()
        }
    }

    /// The side this control is on for the given profile.
    pub fn enabled(&self, profile: PowerProfile) -> bool {
        match profile {
            PowerProfile::Ac => self.ac_enabled,
            PowerProfile::Battery => self.battery_enabled,
        }
    }

    pub fn set_enabled(&mut self, profile: PowerProfile, enabled: bool) {
        match profile {
            PowerProfile::Ac => self.ac_enabled = enabled,
            PowerProfile::Battery => self.battery_enabled = enabled,
        }
    }

    /// The capture that switching to `enabled` replays.
    pub fn capture_for(&self, enabled: bool) -> &str {
        if enabled {
            &self.on_capture
        } else {
            &self.off_capture
        }
    }

    /// Whether both sides are chosen, which is what makes the toggle usable.
    pub fn is_complete(&self) -> bool {
        !self.name.trim().is_empty() && !self.on_capture.is_empty() && !self.off_capture.is_empty()
    }

    /// Carries a control saved before sides were per profile onto both of them,
    /// so upgrading does not silently switch everything off.
    pub(super) fn adopt_legacy_side(&mut self) {
        if !self.legacy_enabled {
            return;
        }
        self.legacy_enabled = false;
        self.ac_enabled = true;
        self.battery_enabled = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toggle() -> CustomToggle {
        CustomToggle {
            name: "Snap Tap".to_string(),
            on_capture: "snap tap on".to_string(),
            off_capture: "snap tap off".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn capture_for_picks_the_side_being_switched_to() {
        assert_eq!(toggle().capture_for(true), "snap tap on");
        assert_eq!(toggle().capture_for(false), "snap tap off");
    }

    #[test]
    fn a_toggle_missing_either_side_is_incomplete() {
        assert!(toggle().is_complete());
        assert!(
            !CustomToggle {
                off_capture: String::new(),
                ..toggle()
            }
            .is_complete()
        );
        assert!(
            !CustomToggle {
                on_capture: String::new(),
                ..toggle()
            }
            .is_complete()
        );
        assert!(
            !CustomToggle {
                name: "  ".to_string(),
                ..toggle()
            }
            .is_complete()
        );
    }

    /// The whole point of the split: a control can be on while plugged in and
    /// off on battery, so setting one side must not disturb the other.
    #[test]
    fn each_profile_remembers_its_own_side() {
        let mut toggle = toggle();
        toggle.set_enabled(PowerProfile::Ac, true);

        assert!(toggle.enabled(PowerProfile::Ac));
        assert!(!toggle.enabled(PowerProfile::Battery));

        toggle.set_enabled(PowerProfile::Battery, true);
        toggle.set_enabled(PowerProfile::Ac, false);

        assert!(!toggle.enabled(PowerProfile::Ac));
        assert!(toggle.enabled(PowerProfile::Battery));
    }

    #[test]
    fn both_profiles_start_switched_off() {
        let toggle = toggle();

        assert!(!toggle.enabled(PowerProfile::Ac));
        assert!(!toggle.enabled(PowerProfile::Battery));
    }

    /// A control that was on before the sides were split stays on, on both, so
    /// upgrading does not quietly turn the user's controls off.
    #[test]
    fn a_control_saved_before_the_split_keeps_its_side_on_both_profiles() {
        let mut toggle = CustomToggle {
            legacy_enabled: true,
            ..toggle()
        };
        toggle.adopt_legacy_side();

        assert!(toggle.enabled(PowerProfile::Ac));
        assert!(toggle.enabled(PowerProfile::Battery));
        assert!(
            !toggle.legacy_enabled,
            "the old field is spent once it has been carried over"
        );
    }

    #[test]
    fn a_control_saved_switched_off_before_the_split_stays_off() {
        let mut toggle = toggle();
        toggle.adopt_legacy_side();

        assert!(!toggle.enabled(PowerProfile::Ac));
        assert!(!toggle.enabled(PowerProfile::Battery));
    }

    /// The old field is read once and never written back, so a config saved by
    /// this version does not keep re-adopting it.
    #[test]
    fn the_old_side_is_never_written_back() {
        let json = serde_json::to_string(&CustomToggle {
            legacy_enabled: true,
            ..toggle()
        })
        .expect("a toggle must serialize");

        assert!(!json.contains("enabled\":true") || json.contains("ac_enabled"));
        assert!(
            !json.contains("\"enabled\""),
            "the pre-split field must not be written again: {json}"
        );
    }
}
