use std::ffi::c_void;
use windows::core::GUID;

/// `AUDCLNT_BUFFERFLAGS_SILENT`. Declared here rather than imported because the
/// flag is a plain documented bit and spelling it out avoids depending on which
/// `windows` feature happens to re-export the constant.
const BUFFER_FLAG_SILENT: u32 = 0x2;
const WAVE_FORMAT_EXTENSIBLE_TAG: u16 = 0xFFFE;
const WAVE_FORMAT_PCM_TAG: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT_TAG: u16 = 3;
/// KSDATAFORMAT_SUBTYPE_IEEE_FLOAT.
const SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
/// KSDATAFORMAT_SUBTYPE_PCM.
const SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);

/// How much audio the shared-mode buffer holds. Comfortably more than one
/// analysis tick, so a late tick does not drop samples.
const CAPTURE_BUFFER_DURATION_100NS: i64 = 200_000; // 20 ms

#[derive(Clone, Copy)]
enum SampleFormat {
    Float32,
    Pcm16,
    Pcm32,
}

/// A WASAPI shared-mode loopback capture of the default render endpoint.
///
/// Polled rather than event-driven: the effect already paces itself at
/// `ANALYSIS_HZ`, so draining whatever has accumulated each tick keeps the
/// capture decoupled from the audio engine's own period.
struct LoopbackCapture {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    sample_rate: u32,
    channels: u16,
    format: SampleFormat,
}

impl LoopbackCapture {
    fn open() -> Result<Self, String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|error| format!("Could not create the device enumerator: {error}"))?;

            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|error| format!("No default render endpoint: {error}"))?;

            let client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|error| format!("Could not activate the audio client: {error}"))?;

            let mix_format = client
                .GetMixFormat()
                .map_err(|error| format!("Could not read the mix format: {error}"))?;
            if mix_format.is_null() {
                return Err("The endpoint reported a null mix format".to_string());
            }

            let described = describe_format(mix_format);

            let initialised = client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                CAPTURE_BUFFER_DURATION_100NS,
                0,
                mix_format,
                None,
            );

            // The mix format is ours to release either way.
            CoTaskMemFree(Some(mix_format as *const c_void));

            initialised
                .map_err(|error| format!("Could not initialise loopback capture: {error}"))?;

            let (sample_rate, channels, format) =
                described.ok_or_else(|| "Unsupported endpoint sample format".to_string())?;

            let capture: IAudioCaptureClient = client
                .GetService()
                .map_err(|error| format!("Could not get the capture service: {error}"))?;

            client
                .Start()
                .map_err(|error| format!("Could not start loopback capture: {error}"))?;

            Ok(Self {
                client,
                capture,
                sample_rate,
                channels,
                format,
            })
        }
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    /// Drains every packet the endpoint has ready into `ring`, downmixed to mono.
    ///
    /// A loopback stream delivers nothing at all while the system is silent, so
    /// an empty drain is the normal quiet case and not an error.
    fn pump(&mut self, ring: &mut SampleRing) -> Result<(), String> {
        unsafe {
            loop {
                let packet = self
                    .capture
                    .GetNextPacketSize()
                    .map_err(|error| format!("GetNextPacketSize failed: {error}"))?;
                if packet == 0 {
                    return Ok(());
                }

                let mut data: *mut u8 = std::ptr::null_mut();
                let mut frames: u32 = 0;
                let mut flags: u32 = 0;
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                    .map_err(|error| format!("GetBuffer failed: {error}"))?;

                if flags & BUFFER_FLAG_SILENT != 0 || data.is_null() {
                    ring.push_silence(frames as usize);
                } else {
                    self.push_frames(ring, data, frames as usize);
                }

                self.capture
                    .ReleaseBuffer(frames)
                    .map_err(|error| format!("ReleaseBuffer failed: {error}"))?;
            }
        }
    }

    /// Downmixes one packet to mono and appends it.
    unsafe fn push_frames(&self, ring: &mut SampleRing, data: *const u8, frames: usize) {
        let channels = self.channels.max(1) as usize;
        let scale = 1.0 / channels as f32;

        for frame in 0..frames {
            let mut sum = 0.0f32;
            for channel in 0..channels {
                let index = frame * channels + channel;
                sum += match self.format {
                    SampleFormat::Float32 => unsafe { *(data as *const f32).add(index) },
                    SampleFormat::Pcm16 => {
                        let raw = unsafe { *(data as *const i16).add(index) };
                        raw as f32 / 32_768.0
                    }
                    SampleFormat::Pcm32 => {
                        let raw = unsafe { *(data as *const i32).add(index) };
                        raw as f32 / 2_147_483_648.0
                    }
                };
            }
            ring.push(sum * scale);
        }
    }
}

impl Drop for LoopbackCapture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

/// Reads rate, channel count and sample encoding out of a `WAVEFORMATEX`,
/// unwrapping `WAVE_FORMAT_EXTENSIBLE` when the endpoint uses it (which shared
/// mode almost always does).
unsafe fn describe_format(format: *const WAVEFORMATEX) -> Option<(u32, u16, SampleFormat)> {
    let header = unsafe { &*format };
    let sample_rate = header.nSamplesPerSec;
    let channels = header.nChannels;
    let bits = header.wBitsPerSample;

    let tag = if header.wFormatTag == WAVE_FORMAT_EXTENSIBLE_TAG {
        let extensible = unsafe { &*(format as *const WAVEFORMATEXTENSIBLE) };
        match extensible.SubFormat {
            SUBTYPE_IEEE_FLOAT => WAVE_FORMAT_IEEE_FLOAT_TAG,
            SUBTYPE_PCM => WAVE_FORMAT_PCM_TAG,
            _ => return None,
        }
    } else {
        header.wFormatTag
    };

    let sample_format = match (tag, bits) {
        (WAVE_FORMAT_IEEE_FLOAT_TAG, 32) => SampleFormat::Float32,
        (WAVE_FORMAT_PCM_TAG, 16) => SampleFormat::Pcm16,
        (WAVE_FORMAT_PCM_TAG, 32) => SampleFormat::Pcm32,
        _ => return None,
    };

    Some((sample_rate, channels, sample_format))
}
