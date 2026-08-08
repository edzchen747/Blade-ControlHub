//! USBPcap driver detection.
//!
//! USBPcap registers a kernel driver service and exposes one `\\.\USBPcapN`
//! capture interface per USB root hub. The registry key marks the driver as
//! installed; the device probe counts usable interfaces.
//!
//! The probe mirrors what USBPcapCMD/Wireshark do to enumerate interfaces:
//! open `\\.\USBPcapN` with zero desired access (no GENERIC_READ — that is
//! denied without elevation even though the device is usable), then issue
//! `IOCTL_USBPCAP_GET_HUB_SYMLINK` to confirm the driver actually serves the
//! interface.

use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use winreg::RegKey;
use winreg::enums::HKEY_LOCAL_MACHINE;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING};
use windows_sys::Win32::System::IO::DeviceIoControl;

use super::{
    HUB_SYMLINK_BUFFER_WORDS, IOCTL_USBPCAP_GET_HUB_SYMLINK, MAX_USBPCAP_INTERFACES,
    usbpcap_interface_name,
};

const USBPCAP_SERVICE_REGISTRY_PATH: &str = r"SYSTEM\CurrentControlSet\Services\USBPcap";
const USBPCAP_STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

pub const USBPCAP_DRIVER_NOT_INSTALLED_LABEL: &str = "USBPcap Driver: not installed";
pub const USBPCAP_DRIVER_INSTALLED_NO_INTERFACES_LABEL: &str =
    "USBPcap Driver: installed (no interfaces available)";
pub const USBPCAP_DOWNLOAD_URL: &str = "https://desowin.org/usbpcap/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbpcapStatus {
    /// Driver service is not registered.
    NotInstalled,
    /// Driver service is registered, with the count of usable capture interfaces.
    Installed { available_interfaces: u32 },
}

/// Whether the USBPcap driver service is registered on this machine.
pub fn usbpcap_driver_installed() -> bool {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(USBPCAP_SERVICE_REGISTRY_PATH)
        .is_ok()
}

/// Count of USBPcap capture interfaces currently usable.
pub fn usbpcap_available_interfaces() -> u32 {
    let mut available = 0;
    for index in 1..=MAX_USBPCAP_INTERFACES {
        match probe_usbpcap_interface(index) {
            Ok(()) => available += 1,
            Err(error) if interface_is_absent(&error) => break,
            Err(_) => {}
        }
    }
    available
}

/// Whether USBPcap is loaded and exposes at least one capture interface.
pub fn usbpcap_available() -> bool {
    usbpcap_available_interfaces() > 0
}

/// Cached driver status for repeated UI reads.
pub fn usbpcap_status() -> UsbpcapStatus {
    let mut cache = USBPCAP_STATUS_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cached) = cache.as_ref()
        && cached.checked_at.elapsed() < USBPCAP_STATUS_REFRESH_INTERVAL
    {
        return cached.status;
    }
    let status = usbpcap_status_unchecked();
    *cache = Some(UsbpcapStatusCache {
        checked_at: Instant::now(),
        status,
    });
    status
}

/// Display label for the driver status line in the Command Lab tab.
pub fn usbpcap_driver_label(status: UsbpcapStatus) -> String {
    match status {
        UsbpcapStatus::NotInstalled => USBPCAP_DRIVER_NOT_INSTALLED_LABEL.to_owned(),
        UsbpcapStatus::Installed {
            available_interfaces: 0,
        } => USBPCAP_DRIVER_INSTALLED_NO_INTERFACES_LABEL.to_owned(),
        UsbpcapStatus::Installed {
            available_interfaces,
        } => {
            let noun = if available_interfaces == 1 {
                "interface"
            } else {
                "interfaces"
            };
            format!("USBPcap Driver: available ({available_interfaces} {noun})")
        }
    }
}

fn usbpcap_status_unchecked() -> UsbpcapStatus {
    if !usbpcap_driver_installed() {
        UsbpcapStatus::NotInstalled
    } else {
        UsbpcapStatus::Installed {
            available_interfaces: usbpcap_available_interfaces(),
        }
    }
}

pub(super) fn probe_usbpcap_interface(index: u32) -> std::io::Result<()> {
    let wide_name: Vec<u16> = usbpcap_interface_name(index)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe { CreateFileW(wide_name.as_ptr(), 0, 0, null_mut(), OPEN_EXISTING, 0, 0) };
    if handle == INVALID_HANDLE_VALUE {
        let error = unsafe { GetLastError() };
        return Err(std::io::Error::from_raw_os_error(error as i32));
    }
    let mut hub_symlink = vec![0u16; HUB_SYMLINK_BUFFER_WORDS];
    let mut bytes_returned = 0u32;
    let ioctl_succeeded = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_USBPCAP_GET_HUB_SYMLINK,
            null(),
            0,
            hub_symlink.as_mut_ptr().cast(),
            (HUB_SYMLINK_BUFFER_WORDS * std::mem::size_of::<u16>()) as u32,
            &mut bytes_returned,
            null_mut(),
        )
    };
    if ioctl_succeeded == 0 {
        let error = unsafe { GetLastError() };
        unsafe { CloseHandle(handle) };
        return Err(std::io::Error::from_raw_os_error(error as i32));
    }
    unsafe { CloseHandle(handle) };
    if bytes_returned == 0 {
        return Err(std::io::Error::other("USBPcap interface reported no hub symlink"));
    }
    Ok(())
}

fn interface_is_absent(error: &std::io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code) if code == ERROR_FILE_NOT_FOUND as i32 || code == ERROR_PATH_NOT_FOUND as i32
    )
}

struct UsbpcapStatusCache {
    checked_at: Instant,
    status: UsbpcapStatus,
}

static USBPCAP_STATUS_CACHE: Mutex<Option<UsbpcapStatusCache>> = Mutex::new(None);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_name_uses_the_win32_device_namespace() {
        assert_eq!(usbpcap_interface_name(1), r"\\.\USBPcap1");
        assert_eq!(usbpcap_interface_name(32), r"\\.\USBPcap32");
    }

    #[test]
    fn driver_label_marks_missing_driver_as_not_installed() {
        assert_eq!(
            usbpcap_driver_label(UsbpcapStatus::NotInstalled),
            USBPCAP_DRIVER_NOT_INSTALLED_LABEL
        );
    }

    #[test]
    fn driver_label_marks_installed_driver_without_interfaces() {
        assert_eq!(
            usbpcap_driver_label(UsbpcapStatus::Installed {
                available_interfaces: 0,
            }),
            USBPCAP_DRIVER_INSTALLED_NO_INTERFACES_LABEL
        );
    }

    #[test]
    fn driver_label_reports_available_interface_count() {
        assert_eq!(
            usbpcap_driver_label(UsbpcapStatus::Installed {
                available_interfaces: 4,
            }),
            "USBPcap Driver: available (4 interfaces)"
        );
    }

    #[test]
    fn driver_label_uses_singular_for_one_interface() {
        assert_eq!(
            usbpcap_driver_label(UsbpcapStatus::Installed {
                available_interfaces: 1,
            }),
            "USBPcap Driver: available (1 interface)"
        );
    }

    #[test]
    fn absent_interface_errors_are_file_not_found_codes() {
        assert!(interface_is_absent(&std::io::Error::from_raw_os_error(
            ERROR_FILE_NOT_FOUND as i32
        )));
        assert!(interface_is_absent(&std::io::Error::from_raw_os_error(
            ERROR_PATH_NOT_FOUND as i32
        )));
    }

    #[test]
    fn non_absent_interface_errors_are_not_treated_as_missing() {
        assert!(!interface_is_absent(&std::io::Error::from_raw_os_error(5)));
        assert!(!interface_is_absent(&std::io::Error::other("probe failed")));
    }
}
