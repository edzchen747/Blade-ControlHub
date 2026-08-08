//! USBPcap integration for Command Lab: driver detection and USB capture.
//!
//! `detect` reports whether the USBPcap driver is installed and how many
//! capture interfaces it exposes. `capture` natively records USB frames from
//! the root hub hosting the Razer EC device, mirroring the USBPcapCMD flow.

pub mod capture;
pub mod detect;

pub use detect::*;

pub(super) const MAX_USBPCAP_INTERFACES: u32 = 32;
pub(super) const RAZER_VID: u16 = 0x1532;

/// CTL_CODE(FILE_DEVICE_UNKNOWN, 0x803, METHOD_BUFFERED, FILE_ANY_ACCESS)
/// from USBPcap's USBPcap.h; returns the root-hub symlink for a filter.
pub(super) const IOCTL_USBPCAP_GET_HUB_SYMLINK: u32 = 0x0022_200C;
pub(super) const HUB_SYMLINK_BUFFER_WORDS: usize = 1024;

pub(super) fn usbpcap_interface_name(index: u32) -> String {
    format!(r"\\.\USBPcap{index}")
}
