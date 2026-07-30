use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::ffi::OsStringExt;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICE_PRIMARY_DEVICE, DISPLAY_DEVICEW,
    EnumDisplayDevicesW, GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFOEXW,
    MonitorFromWindow,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayLayout {
    displays: Vec<DisplayIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DisplayIdentity {
    device_name: String,
    description: String,
    primary: bool,
}

pub fn current_display_layout() -> DisplayLayout {
    let mut displays = Vec::new();
    let mut device_index = 0;

    while let Some(display_device) = enum_display_device(device_index) {
        if attached_to_desktop(&display_device) {
            displays.push(DisplayIdentity {
                device_name: wide_string_until_nul(&display_device.DeviceName),
                description: wide_string_until_nul(&display_device.DeviceString),
                primary: primary_device(&display_device),
            });
        }

        device_index += 1;
    }

    displays.sort_by(|a, b| a.device_name.cmp(&b.device_name));
    DisplayLayout { displays }
}

pub fn display_layout_changed(previous: Option<&DisplayLayout>, current: &DisplayLayout) -> bool {
    previous.is_some_and(|previous| previous != current)
}

pub fn primary_display_device_name() -> Option<OsString> {
    primary_display_device_name_wide().map(|name| wide_slice_to_os_string(&name))
}

pub fn primary_display_device_name_wide() -> Option<Vec<u16>> {
    let mut device_index = 0;
    while let Some(display_device) = enum_display_device(device_index) {
        if attached_to_desktop(&display_device) && primary_device(&display_device) {
            return Some(display_device.DeviceName.to_vec());
        }

        device_index += 1;
    }

    primary_monitor_device_name().map(|name| {
        let mut wide: Vec<u16> = name.encode_wide().collect();
        wide.push(0);
        wide
    })
}

pub fn wide_slice_to_os_string(slice: &[u16]) -> OsString {
    let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    OsString::from_wide(&slice[..len])
}

pub fn wide_string_until_nul(value: &[u16]) -> String {
    wide_slice_to_os_string(value).to_string_lossy().to_string()
}

fn primary_monitor_device_name() -> Option<OsString> {
    unsafe {
        let h_monitor = MonitorFromWindow(HWND::default(), MONITOR_DEFAULTTOPRIMARY);

        let mut monitor_info = MONITORINFOEXW::default();
        monitor_info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

        GetMonitorInfoW(h_monitor, &mut monitor_info.monitorInfo)
            .as_bool()
            .then(|| wide_slice_to_os_string(&monitor_info.szDevice))
    }
}

fn enum_display_device(device_index: u32) -> Option<DISPLAY_DEVICEW> {
    let mut display_device = initialized_display_device();
    unsafe { EnumDisplayDevicesW(None, device_index, &mut display_device, 0).as_bool() }
        .then_some(display_device)
}

fn initialized_display_device() -> DISPLAY_DEVICEW {
    // SAFETY: DISPLAY_DEVICEW is a plain C struct. EnumDisplayDevicesW expects
    // the caller to zero-initialize it and set `cb` before the call.
    let mut display_device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
    display_device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    display_device
}

fn attached_to_desktop(display_device: &DISPLAY_DEVICEW) -> bool {
    (display_device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP) != 0
}

fn primary_device(display_device: &DISPLAY_DEVICEW) -> bool {
    (display_device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE) != 0
}

#[cfg(test)]
mod tests {
    use super::{DisplayIdentity, DisplayLayout, display_layout_changed, wide_string_until_nul};

    #[test]
    fn display_layout_change_is_ignored_without_baseline() {
        let current = DisplayLayout::default();

        assert!(!display_layout_changed(None, &current));
    }

    #[test]
    fn display_layout_change_detects_added_display() {
        let previous = DisplayLayout {
            displays: vec![display("\\\\.\\DISPLAY1", true)],
        };
        let current = DisplayLayout {
            displays: vec![
                display("\\\\.\\DISPLAY1", true),
                display("\\\\.\\DISPLAY2", false),
            ],
        };

        assert!(display_layout_changed(Some(&previous), &current));
    }

    #[test]
    fn display_layout_change_detects_primary_switch() {
        let previous = DisplayLayout {
            displays: vec![
                display("\\\\.\\DISPLAY1", true),
                display("\\\\.\\DISPLAY2", false),
            ],
        };
        let current = DisplayLayout {
            displays: vec![
                display("\\\\.\\DISPLAY1", false),
                display("\\\\.\\DISPLAY2", true),
            ],
        };

        assert!(display_layout_changed(Some(&previous), &current));
    }

    #[test]
    fn wide_string_parser_stops_at_nul() {
        let value = [
            'R' as u16, 'a' as u16, 'z' as u16, 'e' as u16, 'r' as u16, 0, 'X' as u16,
        ];

        assert_eq!(wide_string_until_nul(&value), "Razer");
    }

    fn display(device_name: &str, primary: bool) -> DisplayIdentity {
        DisplayIdentity {
            device_name: device_name.to_string(),
            description: "Generic PnP Monitor".to_string(),
            primary,
        }
    }
}
