use rdev::Key;
use winreg::RegKey;
use winreg::enums::*;

use crate::ui::app_events::OsdEvent;
use crate::ui::tray_app::tray_app;
use crate::win::input::key_map::KeyCombo;

/// Queries the Windows registry to determine if the precision touchpad is enabled.
pub fn get_trackpad_state() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad\Status";

    if let Ok(key) = hkcu.open_subkey(path) {
        match key.get_value::<u32, _>("Enabled") {
            Ok(enabled) => enabled != 0,
            Err(err) => {
                println!("{}", err);
                true
            }
        }
    } else {
        println!("Could not find PrecisionTouchPad registry key.");
        true
    }
}

pub fn toggle_trackpad() {
    KeyCombo::new(&[Key::MetaLeft, Key::ControlLeft, Key::Unknown(135)]).trigger();
    tray_app().send(OsdEvent::Trackpad(get_trackpad_state()));
}
