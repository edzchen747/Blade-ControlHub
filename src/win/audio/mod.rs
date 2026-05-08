mod mute;

pub use mute::{is_audio_muted, toggle_audio_mute};

use std::sync::OnceLock;
use windows::{Win32::Media::Audio::Endpoints::*, Win32::Media::Audio::*, Win32::System::Com::*};

// ── Thread-safe COM pointer wrapper ─────────────────────────────────────────

struct SendSyncCom<T>(pub T);
unsafe impl<T> Send for SendSyncCom<T> {}
unsafe impl<T> Sync for SendSyncCom<T> {}

// ── Audio endpoint type ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioType {
    Speakers = 1,
    Mic = 2,
}

// ── Cached audio endpoint interfaces ────────────────────────────────────────

static MIC_ENDPOINT: OnceLock<Option<SendSyncCom<IAudioEndpointVolume>>> = OnceLock::new();
static SPEAKER_ENDPOINT: OnceLock<Option<SendSyncCom<IAudioEndpointVolume>>> = OnceLock::new();

/// Returns the cached `IAudioEndpointVolume` for the given audio type.
pub(crate) fn get_endpoint(io: AudioType) -> Option<&'static IAudioEndpointVolume> {
    let (lock, direction, role) = match io {
        AudioType::Mic => (&MIC_ENDPOINT, eCapture, eCommunications),
        AudioType::Speakers => (&SPEAKER_ENDPOINT, eRender, eConsole),
    };

    let wrapper = lock.get_or_init(|| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;

        let device = enumerator.GetDefaultAudioEndpoint(direction, role).ok()?;
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;

        Some(SendSyncCom(volume))
    });

    wrapper.as_ref().map(|w| &w.0)
}
