use serde::{Deserialize, Serialize};

/// What the Dashboard's Custom Controls section leaves out.
///
/// Everything the user has recorded or built is shown there by default, so
/// this is a list of the exceptions rather than a list of what to show. A
/// capture recorded after the last time the user edited the section therefore
/// appears on its own, instead of staying invisible until they remember to go
/// back and add it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HiddenDashboardControls {
    /// Command Lab captures, by the name they are saved under.
    pub captures: Vec<String>,
    /// Custom controls, by name.
    pub controls: Vec<String>,
}

impl HiddenDashboardControls {
    pub fn hides_capture(&self, name: &str) -> bool {
        self.captures.iter().any(|hidden| hidden == name)
    }

    pub fn hides_control(&self, name: &str) -> bool {
        self.controls.iter().any(|hidden| hidden == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_hidden_by_default() {
        let hidden = HiddenDashboardControls::default();

        assert!(!hidden.hides_capture("snap tap on"));
        assert!(!hidden.hides_control("Snap Tap"));
    }

    /// A capture and a control may share a name without being the same thing,
    /// so hiding one must not hide the other.
    #[test]
    fn a_capture_and_a_control_are_hidden_separately() {
        let hidden = HiddenDashboardControls {
            captures: vec!["Snap Tap".to_owned()],
            controls: Vec::new(),
        };

        assert!(hidden.hides_capture("Snap Tap"));
        assert!(!hidden.hides_control("Snap Tap"));
    }
}
