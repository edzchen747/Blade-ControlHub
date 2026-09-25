//! The serialisable vocabulary a key can be bound to.
//!
//! The built-in table in `key_map/default_key_map.rs` is a map of boxed
//! closures built once at startup: expressive, but nothing about it can be
//! written to disk or edited from the settings window. These types are the
//! data counterpart — every variant names an action the runtime already knows
//! how to perform, so a binding round-trips through `config.json` and the
//! window without either side holding a closure.

use serde::{Deserialize, Serialize};

/// Bit positions in [`Chord::modifiers`].
pub const MOD_CTRL: u8 = 1 << 0;
pub const MOD_ALT: u8 = 1 << 1;
pub const MOD_SHIFT: u8 = 1 << 2;
pub const MOD_WIN: u8 = 1 << 3;

/// One keystroke: a Windows virtual-key code plus the modifiers held with it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chord {
    pub modifiers: u8,
    pub key: u16,
}

impl Chord {
    /// A chord with no key to press is a half-finished row, not an action.
    pub fn is_complete(&self) -> bool {
        self.key != 0
    }
}

/// Device control actions that can be bound to a key.
///
/// Each variant is a call the runtime already makes from the built-in key map
/// or the tray, so binding one adds no new device behaviour — including the
/// OSD each of them already raises from inside its handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceAction {
    CyclePerfMode,
    CycleRgbEffect,
    CycleRefreshRate,
    CycleBatteryLimit,
    ToggleUnderglow,
    KeyboardLightUp,
    KeyboardLightDown,
    ScreenBrightnessUp,
    ScreenBrightnessDown,
    ToggleMicMute,
    ToggleSpeakerMute,
    ToggleTrackpad,
    TogglePrimaryMultimediaKeys,
    CloseGpuApps,
}

/// Every device action, in the order the settings window lists them.
pub const DEVICE_ACTIONS: &[DeviceAction] = &[
    DeviceAction::CyclePerfMode,
    DeviceAction::CycleRgbEffect,
    DeviceAction::CycleRefreshRate,
    DeviceAction::CycleBatteryLimit,
    DeviceAction::ToggleUnderglow,
    DeviceAction::KeyboardLightUp,
    DeviceAction::KeyboardLightDown,
    DeviceAction::ScreenBrightnessUp,
    DeviceAction::ScreenBrightnessDown,
    DeviceAction::ToggleMicMute,
    DeviceAction::ToggleSpeakerMute,
    DeviceAction::ToggleTrackpad,
    DeviceAction::TogglePrimaryMultimediaKeys,
    DeviceAction::CloseGpuApps,
];

impl DeviceAction {
    /// The label the settings window shows for this action.
    pub fn label(self) -> &'static str {
        match self {
            Self::CyclePerfMode => "Cycle performance mode",
            Self::CycleRgbEffect => "Cycle lighting effect",
            Self::CycleRefreshRate => "Cycle refresh rate",
            Self::CycleBatteryLimit => "Cycle charge limit",
            Self::ToggleUnderglow => "Toggle vapour chamber light",
            Self::KeyboardLightUp => "Keyboard backlight up",
            Self::KeyboardLightDown => "Keyboard backlight down",
            Self::ScreenBrightnessUp => "Screen brightness up",
            Self::ScreenBrightnessDown => "Screen brightness down",
            Self::ToggleMicMute => "Toggle microphone mute",
            Self::ToggleSpeakerMute => "Toggle speaker mute",
            Self::ToggleTrackpad => "Toggle trackpad",
            Self::TogglePrimaryMultimediaKeys => "Toggle media keys on the top row",
            Self::CloseGpuApps => "Close GPU applications",
        }
    }
}

/// What a bound key does when it is pressed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KeyAction {
    /// Swallow the key and do nothing — the way to silence a key outright.
    #[default]
    None,
    /// Send one keystroke, chosen from a list rather than captured — which is
    /// what lets a key the keyboard does not have be mapped at all.
    Key { chord: Chord },
    /// Send a sequence of keystrokes in order, as the user performed them.
    #[serde(alias = "keys")]
    Macro { steps: Vec<Chord> },
    Device { action: DeviceAction },
    /// Show the settings window, or hide it if it is already open.
    ToggleUi,
    LaunchApp {
        path: String,
        #[serde(default)]
        args: String,
        #[serde(default)]
        name: String,
    },
    RunCommand { command: String },
    /// Replay a capture saved on the Command Lab page.
    ReplayCapture { name: String },
}

impl KeyAction {
    /// Whether the action carries everything it needs to run. A row the user
    /// has not finished is stored, so the window can show it again, but it is
    /// never dispatched.
    pub fn is_complete(&self) -> bool {
        match self {
            Self::None | Self::ToggleUi | Self::Device { .. } => true,
            Self::Key { chord } => chord.is_complete(),
            Self::Macro { steps } => !steps.is_empty() && steps.iter().all(Chord::is_complete),
            Self::LaunchApp { path, .. } => !path.trim().is_empty(),
            Self::RunCommand { command } => !command.trim().is_empty(),
            Self::ReplayCapture { name } => !name.trim().is_empty(),
        }
    }
}

/// One row of the Keys page: the key, the user's label for it, and the action.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBinding {
    /// A Razer special-key code, or a Windows virtual-key code for Hypershift.
    pub key_code: u16,
    pub label: String,
    pub action: KeyAction,
}

/// Both tables, in the order the window shows them. A `Vec` rather than a map
/// so the user's row order survives a round-trip through the config file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBindings {
    pub razer: Vec<KeyBinding>,
    pub hypershift: Vec<KeyBinding>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_round_trips_through_json() {
        let bindings = KeyBindings {
            razer: vec![KeyBinding {
                key_code: 0x24,
                label: "Task manager".to_owned(),
                action: KeyAction::Macro {
                    steps: vec![Chord {
                        modifiers: MOD_CTRL | MOD_SHIFT,
                        key: 0x1b,
                    }],
                },
            }],
            hypershift: vec![KeyBinding {
                key_code: 0x4b,
                label: String::new(),
                action: KeyAction::Device {
                    action: DeviceAction::CyclePerfMode,
                },
            }],
        };

        let json = serde_json::to_string(&bindings).expect("serialises");
        let parsed: KeyBindings = serde_json::from_str(&json).expect("parses");

        assert_eq!(parsed, bindings);
    }

    /// The settings window mirrors these shapes by hand in `ui/src/lib/types.ts`,
    /// so the wire format is pinned here rather than left to the derive.
    #[test]
    fn actions_are_tagged_by_kind_in_snake_case() {
        let cases = [
            (KeyAction::None, r#"{"kind":"none"}"#),
            (KeyAction::ToggleUi, r#"{"kind":"toggle_ui"}"#),
            (
                KeyAction::Key {
                    chord: Chord {
                        modifiers: MOD_WIN,
                        key: 0x52,
                    },
                },
                r#"{"kind":"key","chord":{"modifiers":8,"key":82}}"#,
            ),
            (
                KeyAction::Macro {
                    steps: vec![Chord {
                        modifiers: MOD_CTRL,
                        key: 0x43,
                    }],
                },
                r#"{"kind":"macro","steps":[{"modifiers":1,"key":67}]}"#,
            ),
            (
                KeyAction::Device {
                    action: DeviceAction::CycleRgbEffect,
                },
                r#"{"kind":"device","action":"cycle_rgb_effect"}"#,
            ),
            (
                KeyAction::LaunchApp {
                    path: "a.exe".to_owned(),
                    args: String::new(),
                    name: "A".to_owned(),
                },
                r#"{"kind":"launch_app","path":"a.exe","args":"","name":"A"}"#,
            ),
            (
                KeyAction::RunCommand {
                    command: "shutdown".to_owned(),
                },
                r#"{"kind":"run_command","command":"shutdown"}"#,
            ),
            (
                KeyAction::ReplayCapture {
                    name: "Snap".to_owned(),
                },
                r#"{"kind":"replay_capture","name":"Snap"}"#,
            ),
        ];

        for (action, expected) in cases {
            assert_eq!(serde_json::to_string(&action).unwrap(), expected);
        }
    }

    #[test]
    fn every_device_action_is_listed_for_the_window() {
        // A variant missing from DEVICE_ACTIONS would be bindable by hand but
        // never offered, so the catalogue must cover the enum.
        let mut listed: Vec<String> = DEVICE_ACTIONS
            .iter()
            .map(|action| serde_json::to_string(action).unwrap())
            .collect();
        listed.sort();
        listed.dedup();

        assert_eq!(listed.len(), DEVICE_ACTIONS.len(), "duplicate device action");
        assert_eq!(DEVICE_ACTIONS.len(), 14);
    }

    #[test]
    fn a_missing_table_reads_as_an_empty_one() {
        let parsed: KeyBindings = serde_json::from_str("{}").expect("parses");

        assert_eq!(parsed, KeyBindings::default());
    }

    /// Macros were the only key action before they were split from single
    /// keystrokes, so a config written then must still load.
    #[test]
    fn a_macro_still_parses_under_its_former_name() {
        let parsed: KeyAction =
            serde_json::from_str(r#"{"kind":"keys","steps":[{"modifiers":1,"key":67}]}"#)
                .expect("the old tag must still parse");

        assert_eq!(
            parsed,
            KeyAction::Macro {
                steps: vec![Chord {
                    modifiers: MOD_CTRL,
                    key: 0x43,
                }],
            }
        );
    }

    #[test]
    fn half_finished_actions_are_incomplete() {
        assert!(!KeyAction::Macro { steps: Vec::new() }.is_complete());
        assert!(
            !KeyAction::Macro {
                steps: vec![Chord::default()]
            }
            .is_complete()
        );
        assert!(!KeyAction::Key { chord: Chord::default() }.is_complete());
        assert!(
            !KeyAction::LaunchApp {
                path: "  ".to_owned(),
                args: String::new(),
                name: String::new(),
            }
            .is_complete()
        );
        assert!(
            !KeyAction::RunCommand {
                command: String::new()
            }
            .is_complete()
        );
        assert!(KeyAction::None.is_complete());
        assert!(KeyAction::ToggleUi.is_complete());
    }
}
