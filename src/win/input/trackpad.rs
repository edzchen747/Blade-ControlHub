use winreg::RegKey;
use winreg::enums::*;

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
