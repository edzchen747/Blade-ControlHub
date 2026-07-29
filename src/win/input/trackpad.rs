use rdev::Key;
use tracing::warn;
use winreg::RegKey;
use winreg::enums::*;

use crate::ui::app::app;
use crate::ui::app_events::OsdEvent;
use crate::win::input::key_map::KeyCombo;

/// Queries the Windows registry to determine if the precision touchpad is enabled.
pub fn get_trackpad_state() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad\Status";

    if let Ok(key) = hkcu.open_subkey(path) {
        match key.get_value::<u32, _>("Enabled") {
            Ok(enabled) => enabled != 0,
            Err(err) => {
                warn!(error = %err, "Failed to read trackpad Enabled value");
                true
            }
        }
    } else {
        warn!("Could not find PrecisionTouchPad registry key.");
        true
    }
}

pub fn toggle_trackpad() {
    KeyCombo::new(&[Key::MetaLeft, Key::ControlLeft, Key::Unknown(135)]).trigger();
    app(OsdEvent::Trackpad(get_trackpad_state()).into());
}
