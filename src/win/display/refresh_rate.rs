use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

use crate::core::shared_state::SHIFT_PRESSED;
use crate::error::{AppError, AppResult};
use tracing::info;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW, DISP_CHANGE_SUCCESSFUL,
    DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICE_PRIMARY_DEVICE, DISPLAY_DEVICEW,
    DM_DISPLAYFREQUENCY, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE, EnumDisplayDevicesW,
    EnumDisplaySettingsW,
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
        while let Some(display_device) = enum_display_device(device_index) {
            if is_target_display(&display_device) {
                return Some(DisplayManager {
                    device_name: display_device.DeviceName.to_vec(),
                });
            }

            device_index += 1;
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

        while let Some(dev_mode) =
            enum_display_settings(self.pcwstr(), ENUM_DISPLAY_SETTINGS_MODE(mode_index))
        {
            if dev_mode.dmDisplayFrequency > 1 {
                rates.insert(dev_mode.dmDisplayFrequency);
            }
            mode_index += 1;
        }
        rates.into_iter().collect()
    }

    pub fn get_current_rate(&self) -> u32 {
        enum_display_settings(self.pcwstr(), ENUM_CURRENT_SETTINGS)
            .map(|dev_mode| dev_mode.dmDisplayFrequency)
            .unwrap_or(0)
    }

    pub fn set_refresh_rate(&self, new_rate: u32) -> AppResult<()> {
        info!(rate_hz = new_rate, "Setting refresh rate");
        let mut dev_mode =
            enum_display_settings(self.pcwstr(), ENUM_CURRENT_SETTINGS).ok_or_else(|| {
                AppError::Internal(
                    "EnumDisplaySettingsW failed to get current settings".to_string(),
                )
            })?;

        dev_mode.dmDisplayFrequency = new_rate;
        dev_mode.dmFields |= DM_DISPLAYFREQUENCY;

        let result = unsafe {
            ChangeDisplaySettingsExW(
                self.pcwstr(),
                Some(&dev_mode),
                HWND::default(),
                CDS_UPDATEREGISTRY,
                None,
            )
        };
        if result == DISP_CHANGE_SUCCESSFUL {
            Ok(())
        } else {
            Err(AppError::Internal(format!(
                "ChangeDisplaySettingsExW failed for {}Hz: Win32 code {}",
                new_rate, result.0
            )))
        }
    }

    pub fn cycle_refresh_rate(&self) -> AppResult<u32> {
        let supported = self.get_supported_rates();
        if supported.is_empty() {
            return Err(AppError::DisplayNotFound);
        }

        let current = self.get_current_rate();

        let reverse = SHIFT_PRESSED.load(Ordering::SeqCst);
        let next_rate =
            choose_next_rate(&supported, current, reverse).ok_or(AppError::DisplayNotFound)?;

        self.set_refresh_rate(next_rate)?;
        Ok(next_rate)
    }
}

fn initialized_display_device() -> DISPLAY_DEVICEW {
    // SAFETY: DISPLAY_DEVICEW is a plain C struct. EnumDisplayDevicesW expects
    // the caller to zero-initialize it and set `cb` before the call.
    let mut display_device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
    display_device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    display_device
}

fn initialized_dev_mode() -> DEVMODEW {
    // SAFETY: DEVMODEW is a plain C struct. EnumDisplaySettingsW expects
    // `dmSize` to be populated and leaves the remaining fields as output data.
    let mut dev_mode: DEVMODEW = unsafe { std::mem::zeroed() };
    dev_mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    dev_mode
}

fn enum_display_device(device_index: u32) -> Option<DISPLAY_DEVICEW> {
    let mut display_device = initialized_display_device();
    unsafe { EnumDisplayDevicesW(None, device_index, &mut display_device, 0).as_bool() }
        .then_some(display_device)
}

fn enum_display_settings(
    device_name: PCWSTR,
    mode: ENUM_DISPLAY_SETTINGS_MODE,
) -> Option<DEVMODEW> {
    let mut dev_mode = initialized_dev_mode();
    unsafe { EnumDisplaySettingsW(device_name, mode, &mut dev_mode).as_bool() }.then_some(dev_mode)
}

fn is_target_display(display_device: &DISPLAY_DEVICEW) -> bool {
    display_matches_target(
        display_device.StateFlags,
        &wide_string_until_nul(&display_device.DeviceString),
    )
}

fn display_matches_target(flags: u32, device_description: &str) -> bool {
    let is_active = (flags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP) != 0;
    let is_primary = (flags & DISPLAY_DEVICE_PRIMARY_DEVICE) != 0;
    let description = device_description.to_lowercase();
    let is_integrated = description.contains("internal") || description.contains("integrated");
    is_active && (is_primary || is_integrated)
}

fn wide_string_until_nul(value: &[u16]) -> String {
    let end = value.iter().position(|&ch| ch == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

fn choose_next_rate(supported: &[u32], current: u32, reverse: bool) -> Option<u32> {
    if reverse {
        supported
            .iter()
            .rev()
            .find(|&&rate| rate < current)
            .copied()
            .or_else(|| supported.last().copied())
    } else {
        supported
            .iter()
            .find(|&&rate| rate > current)
            .copied()
            .or_else(|| supported.first().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICE_PRIMARY_DEVICE, choose_next_rate,
        display_matches_target, wide_string_until_nul,
    };

    #[test]
    fn choose_next_rate_advances_to_higher_rate() {
        assert_eq!(choose_next_rate(&[60, 120, 144], 60, false), Some(120));
    }

    #[test]
    fn choose_next_rate_wraps_forward_to_first_rate() {
        assert_eq!(choose_next_rate(&[60, 120, 144], 144, false), Some(60));
    }

    #[test]
    fn choose_next_rate_reverses_to_lower_rate() {
        assert_eq!(choose_next_rate(&[60, 120, 144], 144, true), Some(120));
    }

    #[test]
    fn choose_next_rate_wraps_reverse_to_last_rate() {
        assert_eq!(choose_next_rate(&[60, 120, 144], 60, true), Some(144));
    }

    #[test]
    fn choose_next_rate_returns_none_for_empty_rates() {
        assert_eq!(choose_next_rate(&[], 60, false), None);
    }

    #[test]
    fn primary_active_display_is_target() {
        assert!(display_matches_target(
            DISPLAY_DEVICE_ATTACHED_TO_DESKTOP | DISPLAY_DEVICE_PRIMARY_DEVICE,
            "Generic PnP Monitor",
        ));
    }

    #[test]
    fn integrated_active_display_is_target() {
        assert!(display_matches_target(
            DISPLAY_DEVICE_ATTACHED_TO_DESKTOP,
            "Internal Display",
        ));
    }

    #[test]
    fn detached_integrated_display_is_not_target() {
        assert!(!display_matches_target(0, "Internal Display"));
    }

    #[test]
    fn wide_string_parser_stops_at_nul() {
        let value = [
            'R' as u16, 'a' as u16, 'z' as u16, 'e' as u16, 'r' as u16, 0, 'X' as u16,
        ];

        assert_eq!(wide_string_until_nul(&value), "Razer");
    }
}
