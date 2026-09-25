//! What the built-in key map does, described as data.
//!
//! The bindings themselves are boxed closures, so nothing can read a label out
//! of them. The settings window needs one anyway: a custom binding takes
//! precedence over the stock behaviour of its key, and the row that does so
//! says which action it replaced. This table is that description, and a test
//! below keeps it from drifting away from the map it describes.
//!
//! Only the Fn layer is described here. Razer's special keys have no built-in
//! behaviour to override — which key sends which code differs between models,
//! so they are bound entirely by the user.

use crate::core::capabilities;
use crate::win::input::vkey;

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

/// The built-in Fn-layer bindings as the window sees them: code to label.
///
/// A key whose built-in action needs hardware this model does not have is left
/// out: it is free for the user to bind, and the window would otherwise warn
/// that a row overrides something that never ran.
pub fn hypershift_labels() -> std::collections::BTreeMap<u16, String> {
    BUILT_IN_HYPERSHIFT
        .iter()
        .filter(|(key, _, _)| *key != vkey::Key::V || capabilities::has_vapour_chamber())
        .map(|(_, code, label)| (*code, (*label).to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win::input::key_map::KEY_MAP;

    #[test]
    fn every_described_key_is_really_bound() {
        for (key, code, label) in BUILT_IN_HYPERSHIFT {
            assert!(
                KEY_MAP.contains_key(key),
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
            BUILT_IN_HYPERSHIFT.len(),
            "the built-in map and its description have drifted apart"
        );
    }

    #[test]
    fn a_described_key_code_matches_the_key_it_names() {
        for (key, code, _) in BUILT_IN_HYPERSHIFT {
            assert_eq!(vkey::Key::from(*code as u8), *key);
        }
    }

    /// Fn+V is inert on a model without the vent, so the window must not
    /// report it as a key with a built-in action to override.
    #[test]
    fn fn_v_is_only_described_where_the_hardware_is() {
        capabilities::with_vapour_chamber(true, || {
            assert_eq!(
                hypershift_labels().get(&0x56).map(String::as_str),
                Some("Toggle vapour chamber light")
            );
        });

        capabilities::with_vapour_chamber(false, || {
            assert!(!hypershift_labels().contains_key(&0x56));
            assert!(
                hypershift_labels().contains_key(&0x54),
                "the rest of the Fn layer is unaffected"
            );
        });
    }

    /// Every entry left in the map is a key held with Fn. Nothing dispatches
    /// a Razer special key through it any more, so a stray entry would be a
    /// built-in action on a code that means something different per model.
    #[test]
    fn the_map_holds_nothing_but_the_fn_layer() {
        let described: std::collections::HashSet<_> =
            BUILT_IN_HYPERSHIFT.iter().map(|(key, _, _)| *key).collect();

        for key in KEY_MAP.keys() {
            assert!(
                described.contains(key),
                "{key:?} is bound outside the Fn layer"
            );
        }
    }
}
