use std::collections::BTreeSet;
use windows::Win32::Graphics::Gdi::{
    CDS_UPDATEREGISTRY, DEVMODEW, DISP_CHANGE_SUCCESSFUL, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP,
    DISPLAY_DEVICE_PRIMARY_DEVICE, DISPLAY_DEVICEW, DM_DISPLAYFREQUENCY, ENUM_CURRENT_SETTINGS,
    ENUM_DISPLAY_SETTINGS_MODE, EnumDisplayDevicesW, EnumDisplaySettingsW,
};
use windows::core::PCWSTR;

pub struct DisplayManager {
    // We store the wide-string device name (e.g., \\.\DISPLAY1)
    // to reuse in all Win32 calls.
    device_name: Vec<u16>,
}

impl DisplayManager {
    pub fn new() -> Option<Self> {
        let mut device_index = 0;
        loop {
            let mut display_device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
            display_device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;

            unsafe {
                if EnumDisplayDevicesW(None, device_index, &mut display_device, 0).as_bool() {
                    let flags = display_device.StateFlags;

                    let is_active = (flags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP) != 0;
                    let is_primary = (flags & DISPLAY_DEVICE_PRIMARY_DEVICE) != 0;

                    // Check the string for "Internal"
                    let device_string =
                        String::from_utf16_lossy(&display_device.DeviceString).to_lowercase();
                    let is_integrated =
                        device_string.contains("internal") || device_string.contains("integrated");

                    if is_active && (is_primary || is_integrated) {
                        return Some(DisplayManager {
                            device_name: display_device.DeviceName.to_vec(),
                        });
                    }

                    device_index += 1;
                } else {
                    break;
                }
            }
        }
        None
    }

    /// Helper to get the PCWSTR pointer for Win32 calls
    fn pcwstr(&self) -> PCWSTR {
        PCWSTR::from_raw(self.device_name.as_ptr())
    }

    pub fn get_supported_rates(&self) -> Vec<u32> {
        let mut rates = BTreeSet::new();
        let mut mode_index = 0;

        loop {
            let mut dev_mode: DEVMODEW = unsafe { std::mem::zeroed() };
            dev_mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;

            unsafe {
                if EnumDisplaySettingsW(
                    self.pcwstr(),
                    ENUM_DISPLAY_SETTINGS_MODE(mode_index),
                    &mut dev_mode,
                )
                .as_bool()
                {
                    if dev_mode.dmDisplayFrequency > 1 {
                        rates.insert(dev_mode.dmDisplayFrequency);
                    }
                    mode_index += 1;
                } else {
                    break;
                }
            }
        }
        rates.into_iter().collect()
    }

    pub fn get_current_rate(&self) -> u32 {
        let mut dev_mode: DEVMODEW = unsafe { std::mem::zeroed() };
        dev_mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;

        unsafe {
            if EnumDisplaySettingsW(self.pcwstr(), ENUM_CURRENT_SETTINGS, &mut dev_mode).as_bool() {
                dev_mode.dmDisplayFrequency
            } else {
                0
            }
        }
    }

    pub fn set_refresh_rate(&self, new_rate: u32) -> Result<(), i32> {
        println!("Setting refresh rate to {} Hz", new_rate);
        let mut dev_mode: DEVMODEW = unsafe { std::mem::zeroed() };
        dev_mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;

        unsafe {
            // Get current settings to keep current resolution
            if !EnumDisplaySettingsW(self.pcwstr(), ENUM_CURRENT_SETTINGS, &mut dev_mode).as_bool()
            {
                return Err(-1);
            }

            dev_mode.dmDisplayFrequency = new_rate;
            dev_mode.dmFields |= DM_DISPLAYFREQUENCY;

            // CDS_UPDATEREGISTRY makes the change persistent (survives reboot).
            let result = windows::Win32::Graphics::Gdi::ChangeDisplaySettingsW(
                Some(&dev_mode),
                CDS_UPDATEREGISTRY,
            );

            if result == DISP_CHANGE_SUCCESSFUL {
                Ok(())
            } else {
                Err(result.0)
            }
        }
    }

    pub fn cycle_refresh_rate(&self) -> Result<u32, String> {
        let supported = self.get_supported_rates();
        if supported.is_empty() {
            return Err("No supported refresh rates found.".to_string());
        }

        let current = self.get_current_rate();

        // Find the first rate in the sorted list that is higher than our current rate
        let next_rate = match supported.iter().find(|&&rate| rate > current) {
            Some(&higher_rate) => higher_rate,
            None => *supported.first().unwrap(),
        };

        match self.set_refresh_rate(next_rate) {
            Ok(_) => Ok(next_rate),
            Err(e) => Err(format!("Win32 Error Code: {}", e)),
        }
    }
}
