use serde::{Deserialize, Serialize};

/// A two-state control the user builds on the Command Lab page out of two
/// saved captures: one is replayed to switch it on, the other to switch it off.
///
/// The captures are named rather than copied, so re-recording one under the
/// same name changes what the toggle does without touching the toggle.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomToggle {
    pub name: String,
    /// A capture name, or empty while the user has not chosen one yet.
    pub on_capture: String,
    pub off_capture: String,
    /// Which side was replayed last, so the control reads the way it was left.
    pub enabled: bool,
}

impl CustomToggle {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toggle() -> CustomToggle {
        CustomToggle {
            name: "Snap Tap".to_string(),
            on_capture: "snap tap on".to_string(),
            off_capture: "snap tap off".to_string(),
            enabled: false,
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
}
