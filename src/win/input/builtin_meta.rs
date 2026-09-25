//! What the built-in key map does, described as data.
//!
//! The bindings themselves are boxed closures, so nothing can read a label out
//! of them. The settings window needs one anyway: a custom binding takes
//! precedence over the stock behaviour of its key, and the row that does so
//! says which action it replaced. This table is that description, and a test
//! below keeps it from drifting away from the map it describes.

use crate::win::input::{razer_key, vkey};

/// Razer special keys the built-in map handles, with their key codes.
pub const BUILT_IN_RAZER: &[(razer_key::Key, u16, &str)] = &[
    (razer_key::Key::Mic, 0xd4, "Toggle microphone mute"),
    (razer_key::Key::Trackpad, 0xdd, "Toggle trackpad"),
    (razer_key::Key::Perf, 0xd3, "Cycle performance mode"),
    (razer_key::Key::CoPilot, 0xd2, "Cycle lighting effect"),
    (razer_key::Key::Home, 0xd5, "Home"),
    (razer_key::Key::Up, 0xd6, "Arrow up"),
    (razer_key::Key::PgUp, 0xd7, "Page up"),
    (razer_key::Key::Left, 0xd8, "Arrow left"),
    (razer_key::Key::Right, 0xd9, "Arrow right"),
    (razer_key::Key::End, 0xda, "End"),
    (razer_key::Key::Down, 0xdb, "Arrow down"),
    (razer_key::Key::PgDn, 0xdc, "Page down"),
];

/// Keys the built-in map handles while Fn is held.
///
/// The top row is conditional — F1–F12 swap between their media action and
/// plain function keys with the "media keys on the top row" setting — but from
/// the window's point of view the key is spoken for either way.
pub const BUILT_IN_HYPERSHIFT: &[(vkey::Key, u16, &str)] = &[
    (vkey::Key::B, 0x42, "Cycle charge limit"),
    (vkey::Key::P, 0x50, "Cycle performance mode"),
    (vkey::Key::R, 0x52, "Cycle refresh rate"),
    (vkey::Key::T, 0x54, "Toggle trackpad"),
    (vkey::Key::V, 0x56, "Toggle vapour chamber light"),
    (vkey::Key::F1, 0x70, "Mute"),
    (vkey::Key::F2, 0x71, "Volume down"),
    (vkey::Key::F3, 0x72, "Volume up"),
    (vkey::Key::F4, 0x73, "Project to a second screen"),
    (vkey::Key::F5, 0x74, "Previous track"),
    (vkey::Key::F6, 0x75, "Play or pause"),
    (vkey::Key::F7, 0x76, "Next track"),
    (vkey::Key::F8, 0x77, "Screen brightness down"),
    (vkey::Key::F9, 0x78, "Screen brightness up"),
    (vkey::Key::F10, 0x79, "Keyboard backlight down"),
    (vkey::Key::F11, 0x7a, "Keyboard backlight up"),
    (vkey::Key::F12, 0x7b, "Print screen"),
];

/// The built-in Razer bindings as the window sees them: code to label.
pub fn razer_labels() -> std::collections::BTreeMap<u16, String> {
    BUILT_IN_RAZER
        .iter()
        .map(|(_, code, label)| (*code, (*label).to_owned()))
        .collect()
}

/// The built-in Fn-layer bindings as the window sees them: code to label.
pub fn hypershift_labels() -> std::collections::BTreeMap<u16, String> {
    BUILT_IN_HYPERSHIFT
        .iter()
        .map(|(_, code, label)| (*code, (*label).to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win::input::KeyType;
    use crate::win::input::key_map::KEY_MAP;

    #[test]
    fn every_described_key_is_really_bound() {
        for (key, code, label) in BUILT_IN_RAZER {
            assert!(
                KEY_MAP.contains_key(&KeyType::from(*key)),
                "0x{code:02x} ({label}) is described but not bound"
            );
        }
        for (key, code, label) in BUILT_IN_HYPERSHIFT {
            assert!(
                KEY_MAP.contains_key(&KeyType::from(*key)),
                "0x{code:02x} ({label}) is described but not bound"
            );
        }
    }

    /// Together with the test above, this pins the two tables to the same set:
    /// a binding added to the map without a description here fails the count.
    #[test]
    fn every_bound_key_is_described() {
        assert_eq!(
            KEY_MAP.len(),
            BUILT_IN_RAZER.len() + BUILT_IN_HYPERSHIFT.len(),
            "the built-in map and its description have drifted apart"
        );
    }

    #[test]
    fn a_described_key_code_matches_the_key_it_names() {
        for (key, code, _) in BUILT_IN_RAZER {
            assert_eq!(razer_key::Key::from(*code as u8), *key);
        }
        for (key, code, _) in BUILT_IN_HYPERSHIFT {
            assert_eq!(vkey::Key::from(*code as u8), *key);
        }
    }
}
