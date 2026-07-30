use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

use crate::core::shared_state::SHIFT_PRESSED;
use crate::error::{AppError, AppResult};
use crate::win::display::topology::{primary_display_device_name_wide, wide_string_until_nul};
use tracing::info;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW, DISP_CHANGE_SUCCESSFUL,
    DM_DISPLAYFREQUENCY, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE, EnumDisplaySettingsW,
};
use windows::core::PCWSTR;

pub struct DisplayManager {
    // We store the wide-string device name (e.g., \\.\DISPLAY1)
    // to reuse in all Win32 calls.
    device_name: Vec<u16>,
}

impl DisplayManager {
    pub fn new() -> Option<Self> {
        Self::primary_device_name().map(|device_name| DisplayManager { device_name })
    }

    pub fn refresh_primary(&mut self) -> AppResult<()> {
        let device_name = Self::primary_device_name().ok_or(AppError::DisplayNotFound)?;
        info!(
            display = %wide_string_until_nul(&device_name),
            "Refreshing cached primary display"
        );
        self.device_name = device_name;
        Ok(())
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

    fn primary_device_name() -> Option<Vec<u16>> {
        primary_display_device_name_wide()
    }
}

fn initialized_dev_mode() -> DEVMODEW {
    // SAFETY: DEVMODEW is a plain C struct. EnumDisplaySettingsW expects
    // `dmSize` to be populated and leaves the remaining fields as output data.
    let mut dev_mode: DEVMODEW = unsafe { std::mem::zeroed() };
    dev_mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    dev_mode
}

fn enum_display_settings(
    device_name: PCWSTR,
    mode: ENUM_DISPLAY_SETTINGS_MODE,
) -> Option<DEVMODEW> {
    let mut dev_mode = initialized_dev_mode();
    unsafe { EnumDisplaySettingsW(device_name, mode, &mut dev_mode).as_bool() }.then_some(dev_mode)
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
    use super::choose_next_rate;
    use crate::win::display::topology::wide_string_until_nul;

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
    fn wide_string_parser_stops_at_nul() {
        let value = [
            'R' as u16, 'a' as u16, 'z' as u16, 'e' as u16, 'r' as u16, 0, 'X' as u16,
        ];

        assert_eq!(wide_string_until_nul(&value), "Razer");
    }
}
