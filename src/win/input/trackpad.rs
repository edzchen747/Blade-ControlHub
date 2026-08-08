use rdev::Key;
use tracing::warn;
use winreg::RegKey;
use winreg::enums::*;

use crate::ui::app::app;
use crate::ui::app_events::OsdEvent;
use crate::win::input::key_map::KeyCombo;

const TRACKPAD_STATUS_REGISTRY_PATH: &str =
    r"Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad\Status";
const TRACKPAD_ENABLED_REGISTRY_VALUE: &str = "Enabled";
const DEFAULT_TRACKPAD_STATE_WHEN_UNKNOWN: bool = true;

pub fn get_trackpad_state() -> bool {
    match read_trackpad_registry_value() {
        Ok(enabled) => trackpad_state_from_registry_value(enabled),
        Err(error) => {
            warn!(%error, "Failed to read PrecisionTouchPad state; assuming enabled");
            DEFAULT_TRACKPAD_STATE_WHEN_UNKNOWN
        }
    }
}

fn read_trackpad_registry_value() -> std::io::Result<u32> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey(TRACKPAD_STATUS_REGISTRY_PATH)?;
    key.get_value::<u32, _>(TRACKPAD_ENABLED_REGISTRY_VALUE)
}

fn trackpad_state_from_registry_value(enabled: u32) -> bool {
    enabled != 0
}

pub fn toggle_trackpad() {
    trackpad_toggle_combo().trigger();
    app(OsdEvent::Trackpad(get_trackpad_state()).into());
}

fn trackpad_toggle_combo() -> KeyCombo {
    KeyCombo::new(&[Key::MetaLeft, Key::ControlLeft, Key::Unknown(135)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trackpad_registry_zero_maps_to_disabled() {
        assert!(!trackpad_state_from_registry_value(0));
    }

    #[test]
    fn trackpad_registry_nonzero_maps_to_enabled() {
        assert!(trackpad_state_from_registry_value(1));
        assert!(trackpad_state_from_registry_value(u32::MAX));
    }

    #[test]
    fn trackpad_toggle_combo_uses_windows_control_touchpad_key() {
        let combo = trackpad_toggle_combo();
        let keys = combo.into_iter().copied().collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec![Key::MetaLeft, Key::ControlLeft, Key::Unknown(135)]
        );
    }
}
