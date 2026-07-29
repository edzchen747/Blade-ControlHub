mod mute;

pub use mute::{is_audio_muted, toggle_audio_mute};

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

/// Creates an `IAudioEndpointVolume` for the calling thread.
pub(crate) fn create_endpoint(io: AudioType) -> windows::core::Result<IAudioEndpointVolume> {
    let selection = io.endpoint_selection();

    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        let device = enumerator.GetDefaultAudioEndpoint(selection.direction, selection.role)?;
        device.Activate(CLSCTX_ALL, None)
    }
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
