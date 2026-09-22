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
    /// Samples held back so the visuals line up with what is actually heard.
    ///
    /// Loopback taps the render stream, which is upstream of everything between
    /// the mixer and the ear. On Bluetooth that gap is large enough to see - the
    /// keyboard reacts before the sound arrives - so the audio is delayed by
    /// whatever the endpoint reports before it is analysed. Delaying the samples
    /// rather than the rendered frames is the same thing and far cheaper: one
    /// queue of floats instead of a history of matrices.
    delay: OutputDelay,
    /// The endpoint this capture is bound to.
    ///
    /// Held so the loop can notice the default moving elsewhere. A capture does
    /// not fail when that happens - the endpoint stays valid and just stops
    /// carrying audio - so the id is the only thing that tells us apart from a
    /// genuinely silent system.
    device_id: String,
}

impl LoopbackCapture {
    fn open() -> Result<Self, String> {
        unsafe {
            // The same endpoint the mute indicator reads, by construction.
            let device = default_endpoint(AudioType::Speakers)
                .map_err(|error| format!("No default render endpoint: {error}"))?;

            let device_id = device
                .GetId()
                .ok()
                .map(|id| take_com_string(id))
                .unwrap_or_default();

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

            let delay_samples = reported_latency_samples(sample_rate);
            if delay_samples > 0 {
                info!(
                    delay_ms = delay_samples as f64 * 1000.0 / sample_rate as f64,
                    "Offsetting the visualiser by the endpoint's reported output latency"
                );
            }

            Ok(Self {
                client,
                capture,
                sample_rate,
                channels,
                format,
                delay: OutputDelay::new(delay_samples),
                device_id,
            })
        }
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Whether the system's default output has moved to a different endpoint.
    ///
    /// An unreadable default reports `false` rather than `true`: a transient
    /// failure to enumerate is not evidence that the device changed, and acting
    /// on it would tear down a capture that is working.
    fn endpoint_changed(&self) -> bool {
        match default_endpoint_id(AudioType::Speakers) {
            Some(current) => current != self.device_id,
            None => false,
        }
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
                    for _ in 0..frames {
                        self.emit(ring, 0.0);
                    }
                } else {
                    self.push_frames(ring, data, frames as usize);
                }

                self.capture
                    .ReleaseBuffer(frames)
                    .map_err(|error| format!("ReleaseBuffer failed: {error}"))?;
            }
        }
    }

    fn emit(&mut self, ring: &mut SampleRing, sample: f32) {
        self.delay.push(ring, sample);
    }

    /// Downmixes one packet to mono and appends it.
    unsafe fn push_frames(&mut self, ring: &mut SampleRing, data: *const u8, frames: usize) {
        let channels = self.channels.max(1) as usize;
        let scale = 1.0 / channels as f32;
        let format = self.format;

        for frame in 0..frames {
            let mut sum = 0.0f32;
            for channel in 0..channels {
                let index = frame * channels + channel;
                sum += match format {
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
            self.emit(ring, sum * scale);
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

/// Holds audio back so the visuals line up with what is actually heard.
///
/// Its own type so the behaviour can be tested: a `LoopbackCapture` needs a
/// real endpoint and cannot be built in a unit test, but this can.
struct OutputDelay {
    queue: VecDeque<f32>,
    samples: usize,
}

impl OutputDelay {
    fn new(samples: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            samples,
        }
    }

    /// Appends one sample, releasing the one from `samples` ago.
    ///
    /// Nothing reaches the ring until the queue has filled, so the visuals
    /// start late by exactly the delay rather than racing ahead and settling.
    fn push(&mut self, ring: &mut SampleRing, sample: f32) {
        if self.samples == 0 {
            ring.push(sample);
            return;
        }

        self.queue.push_back(sample);
        while self.queue.len() > self.samples {
            if let Some(delayed) = self.queue.pop_front() {
                ring.push(delayed);
            }
        }
    }
}

/// How far to hold audio back so the visuals match what is heard, in samples.
///
/// What the endpoint reports is preferred over anything assumed. A wired
/// endpoint here reports 0 ms and gets no offset at all. Only when a *Bluetooth*
/// endpoint reports nothing does `BLUETOOTH_FALLBACK_OFFSET_MS` stand in, on the
/// grounds that a silent wired device genuinely has no delay while a silent
/// Bluetooth one is a gap in what Windows exposes.
///
/// Clamped to a second, because a nonsense report should cost a bounded queue
/// and a visibly late visualiser rather than unbounded memory.
fn reported_latency_samples(sample_rate: u32) -> usize {
    let Some(info) = endpoint_info(AudioType::Speakers) else {
        return 0;
    };

    let latency_ms = match info.stream_latency_ms {
        Some(reported) if reported > 0.0 => reported,
        _ if info.is_bluetooth() => BLUETOOTH_FALLBACK_OFFSET_MS,
        _ => return 0,
    };

    let samples = (latency_ms / 1000.0 * f64::from(sample_rate)).round();
    (samples as usize).min(sample_rate as usize)
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
