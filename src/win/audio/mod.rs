pub mod bloom;
mod endpoint_watch;
mod mute;

pub use bloom::AudioBloomEffect;
pub use mute::{is_audio_muted, toggle_audio_mute};

pub(crate) use endpoint_watch::{DefaultEndpointWatcher, endpoint_info};

use tracing::warn;
use windows::{
    Win32::Foundation::RPC_E_CHANGED_MODE, Win32::Media::Audio::Endpoints::*,
    Win32::Media::Audio::*, Win32::System::Com::*,
};

// ── Audio endpoint type ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioType {
    Speakers = 1,
    Mic = 2,
}

struct EndpointSelection {
    direction: EDataFlow,
    role: ERole,
}

impl AudioType {
    fn endpoint_selection(self) -> EndpointSelection {
        match self {
            AudioType::Mic => EndpointSelection {
                direction: eCapture,
                role: eCommunications,
            },
            AudioType::Speakers => EndpointSelection {
                direction: eRender,
                role: eConsole,
            },
        }
    }
}

struct ComApartment {
    owns_apartment: bool,
}

impl ComApartment {
    fn initialize_mta() -> windows::core::Result<Self> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            return Ok(Self {
                owns_apartment: true,
            });
        }

        if result == RPC_E_CHANGED_MODE {
            warn!(
                "COM is already initialized with a different apartment model; reusing existing apartment for audio endpoint access"
            );
            return Ok(Self {
                owns_apartment: false,
            });
        }

        result.ok()?;
        Ok(Self {
            owns_apartment: true,
        })
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.owns_apartment {
            unsafe { CoUninitialize() };
        }
    }
}

/// The device Windows currently treats as the default for `io`.
///
/// Resolved fresh on every call rather than cached, which is what lets
/// everything built on it follow the user changing their output device without
/// having to be told. One definition of "the output device" for the whole app:
/// the mute indicator and the Equalizer's loopback capture must agree on which
/// endpoint that is, and they only do so as long as the selection lives here
/// and not at each call site.
pub(crate) fn default_endpoint(io: AudioType) -> windows::core::Result<IMMDevice> {
    let selection = io.endpoint_selection();

    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        enumerator.GetDefaultAudioEndpoint(selection.direction, selection.role)
    }
}

/// The id of the current default endpoint for `io`.
///
/// Stable for a given device, so comparing it against a stored copy is how a
/// long-lived capture notices the default has moved out from under it. A
/// capture bound to the old endpoint keeps succeeding and simply returns
/// nothing, so there is no error to watch for instead.
pub(crate) fn default_endpoint_id(io: AudioType) -> Option<String> {
    let device = default_endpoint(io).ok()?;
    unsafe { device.GetId().ok().map(|id| take_com_string(id)) }
}

/// Reads a COM-allocated wide string and frees it.
pub(crate) unsafe fn take_com_string(value: windows::core::PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }

    unsafe {
        let text = value.to_string().unwrap_or_default();
        CoTaskMemFree(Some(value.0 as *const std::ffi::c_void));
        text
    }
}

/// Creates an `IAudioEndpointVolume` for the calling thread.
pub(crate) fn create_endpoint(io: AudioType) -> windows::core::Result<IAudioEndpointVolume> {
    let device = default_endpoint(io)?;
    unsafe { device.Activate(CLSCTX_ALL, None) }
}

pub(crate) fn with_endpoint<R>(
    io: AudioType,
    f: impl FnOnce(&IAudioEndpointVolume) -> windows::core::Result<R>,
) -> windows::core::Result<R> {
    let _apartment = ComApartment::initialize_mta()?;
    let endpoint = create_endpoint(io)?;

    f(&endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speakers_use_render_console_endpoint() {
        let selection = AudioType::Speakers.endpoint_selection();

        assert_eq!(selection.direction, eRender);
        assert_eq!(selection.role, eConsole);
    }

    #[test]
    fn mic_uses_capture_communications_endpoint() {
        let selection = AudioType::Mic.endpoint_selection();

        assert_eq!(selection.direction, eCapture);
        assert_eq!(selection.role, eCommunications);
    }
}
