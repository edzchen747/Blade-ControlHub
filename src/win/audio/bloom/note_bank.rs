/// A circular buffer of the most recent mono samples.
///
/// Sized to `MAX_WINDOW` because that is the longest Goertzel window any bin
/// asks for, so the lowest bins always have their full history available.
struct SampleRing {
    buffer: Vec<f32>,
    write: usize,
    /// Total samples ever written. The onset bank steps through the stream in
    /// fixed hops rather than once per rendered frame, and needs this to know
    /// how much new audio has arrived.
    written: u64,
}

impl SampleRing {
    fn new() -> Self {
        Self {
            buffer: vec![0.0; MAX_WINDOW],
            write: 0,
            written: 0,
        }
    }

    fn push(&mut self, sample: f32) {
        self.buffer[self.write] = sample;
        self.write = (self.write + 1) % MAX_WINDOW;
        self.written += 1;
    }

    fn push_silence(&mut self, frames: usize) {
        for _ in 0..frames.min(MAX_WINDOW) {
            self.push(0.0);
        }
    }

    fn written(&self) -> u64 {
        self.written
    }

    /// Sample `index` of a `window` that ended `ago` samples before the newest
    /// one, oldest first.
    ///
    /// Lets the onset bank walk the stream in fixed hops and catch up on any it
    /// missed, instead of only ever seeing the instant a frame was rendered.
    fn get_ending(&self, ago: usize, window: usize, index: usize) -> f32 {
        let end = (self.write + MAX_WINDOW - ago % MAX_WINDOW) % MAX_WINDOW;
        let start = (end + MAX_WINDOW - window) % MAX_WINDOW;
        self.buffer[(start + index) % MAX_WINDOW]
    }
}

/// One Goertzel filter.
struct BinSpec {
    frequency: f64,
    window: usize,
    coeff: f32,
    scale: f32,
}

/// A bank of Goertzel filters grouped into log-spaced frequency bands.
///
/// Goertzel rather than an FFT because the band count is small and each filter
/// carries its own window length, so the low bands integrate over a long window
/// while the high bands stay responsive. An FFT would need several transform
/// sizes to do the same.
///
/// Every band runs its **own** AGC. One follower across the whole spectrum lets
/// the bass swamp everything and leaves the upper bands pinned at the floor,
/// which is useless for a display where each band needs its own full range.
struct NoteBank {
    ring: SampleRing,
    bins: Vec<BinSpec>,
    hann: Vec<f32>,
    /// Peak follower per band.
    peaks: Vec<f32>,
    /// Smoothed, normalised level per band — what the quadrants draw.
    levels: Vec<f32>,
    /// The same measurement before smoothing. The bass bar runs its own, faster
    /// envelope off this, so it can fall quickly without dragging the quadrant
    /// bands down with it.
    raw: Vec<f32>,
    /// Kick detection, on its own fixed-window bin bank.
    onset: OnsetBank,
    /// Instantaneous magnitude per band, before any AGC.
    ///
    /// Hit detection needs these rather than the normalised levels: per-band AGC
    /// stretches every band to full scale, which throws away both the absolute
    /// size of a sound and how it compares to the other bands — the only two
    /// things that tell a kick drum from a vocal transient.
    power: Vec<f32>,
    bands: usize,
    sample_rate: f64,
}

impl NoteBank {
    fn new(sample_rate: f64, bands: usize) -> Self {
        let bands = bands.max(1);
        let mut bins = Vec::with_capacity(bands * BINS_PER_BAND);

        // Band 0 is pinned to an absolute range, so what counts as bass never
        // shifts with the keyboard width and speech cannot creep into it.
        push_log_bins(
            &mut bins,
            sample_rate,
            BASS_BAND_MIN_HZ,
            BASS_BAND_MAX_HZ,
            BINS_PER_BAND,
        );

        // Everything above it splits the rest of the spectrum evenly in log
        // frequency, so each of those bands covers an equal ratio.
        let upper_bins = bands.saturating_sub(1) * BINS_PER_BAND;
        push_log_bins(
            &mut bins,
            sample_rate,
            BASS_BAND_MAX_HZ,
            SPECTRUM_MAX_HZ,
            upper_bins,
        );

        let hann = (0..MAX_WINDOW)
            .map(|i| {
                let phase = std::f64::consts::TAU * i as f64 / MAX_WINDOW as f64;
                (0.5 - 0.5 * phase.cos()) as f32
            })
            .collect();

        Self {
            ring: SampleRing::new(),
            bins,
            hann,
            peaks: vec![AGC_FLOOR; bands],
            levels: vec![0.0; bands],
            raw: vec![0.0; bands],
            onset: OnsetBank::new(sample_rate),
            power: vec![0.0; bands],
            bands,
            sample_rate,
        }
    }

    fn samples_mut(&mut self) -> &mut SampleRing {
        &mut self.ring
    }

    /// The smoothed 0..1 level of every band, lowest frequency first.
    fn levels(&self) -> &[f32] {
        &self.levels
    }

    /// The same levels before smoothing, for anything wanting its own envelope.
    fn raw_levels(&self) -> &[f32] {
        &self.raw
    }

    /// Instantaneous per-band magnitude, before any AGC.
    ///
    /// Hit detection reads the field directly; this is for the bench and the
    /// tests that pin the kick-versus-voice separation it is tuned against.
    fn power(&self) -> &[f32] {
        &self.power
    }

    /// The raw and normalised onset detection functions, for the bench.
    fn onset_odf(&self) -> f32 {
        self.onset.odf()
    }

    fn onset_strength(&self) -> f32 {
        self.onset.strength()
    }

    /// Which low band currently has the floor, and whether the gap fallback has
    /// opened it to the others. For the bench: without these the dump shows
    /// that a band was chosen but never why.
    fn onset_selected_band(&self) -> usize {
        self.onset.selected_band()
    }

    fn onset_in_fallback(&self) -> bool {
        self.onset.in_fallback()
    }

    fn onset_band_count(&self) -> usize {
        self.onset.band_count()
    }

    fn onset_band_energy(&self, index: usize) -> f32 {
        self.onset.band_energy(index)
    }

    fn onset_band_flux(&self, index: usize) -> f32 {
        self.onset.band_flux_value(index)
    }

    fn onset_band_strength(&self, index: usize) -> f32 {
        self.onset.band_strength(index)
    }

    /// Whether the last tick carried a kick-drum hit.
    fn bass_hit(&self) -> bool {
        self.onset.hit()
    }

    fn lowest_frequency(&self) -> f64 {
        self.bins.first().map_or(0.0, |bin| bin.frequency)
    }

    fn highest_frequency(&self) -> f64 {
        self.bins.last().map_or(0.0, |bin| bin.frequency)
    }

    /// Measures every band and folds the result into the smoothed levels.
    fn analyze(&mut self, dt: f32) {
        for band in 0..self.bands {
            // The loudest bin in the band, not the mean: an octave-wide band is
            // far too broad for averaging to stay reactive, and a single strong
            // note would be flattened into the noise either side of it.
            let mut loudest = 0.0f32;
            for offset in 0..BINS_PER_BAND {
                let bin = &self.bins[band * BINS_PER_BAND + offset];
                loudest = loudest.max(goertzel(&self.ring, &self.hann, bin));
            }
            self.power[band] = loudest;

            let follower = if loudest > self.peaks[band] {
                AGC_ATTACK
            } else {
                AGC_RELEASE
            };
            self.peaks[band] += (loudest - self.peaks[band]) * follower;

            // The bass band holds a much higher floor, so leakage cannot be
            // stretched up into a phantom kick.
            let floor = if band == 0 { BASS_AGC_FLOOR } else { AGC_FLOOR };
            self.peaks[band] = self.peaks[band].max(floor);

            // Drawn in dB, so the span between silence and the band's own peak
            // is spread perceptually instead of crowding everything quiet into
            // the bottom of a linear scale.
            let gated = decibels_below(loudest, self.peaks[band]);
            self.raw[band] = gated;

            let smoothing = if gated > self.levels[band] {
                LEVEL_ATTACK
            } else {
                LEVEL_RELEASE
            };
            let step = (smoothing * dt / REFERENCE_TICK).clamp(0.0, 1.0);
            self.levels[band] += (gated - self.levels[band]) * step;
        }

        self.detect_bass_hit();
    }

    /// Picks kick drums out of the low end.
    ///
    /// The work is in `OnsetBank`, which runs its own fixed-window bin bank: the
    /// display bins here each use a different window length, which suits a
    /// spectrum but makes flux between them meaningless.
    fn detect_bass_hit(&mut self) {
        self.onset.analyze(&self.ring, &self.hann, self.sample_rate);
    }
}

/// Appends `count` filters log-spaced from `start` up towards `end`.
///
/// `end` is the exclusive top: the last filter sits one step below it, so two
/// adjacent calls tile the spectrum without doubling up on the boundary.
fn push_log_bins(
    bins: &mut Vec<BinSpec>,
    sample_rate: f64,
    start: f64,
    end: f64,
    count: usize,
) {
    if count == 0 {
        return;
    }

    let ratio = (end / start).powf(1.0 / count as f64);
    for index in 0..count {
        let frequency = start * ratio.powi(index as i32);
        // Each filter spans a fixed number of periods of its own frequency, so
        // the low bins integrate over a long window and the high ones stay
        // responsive.
        let window = ((PERIODS_PER_WINDOW * sample_rate / frequency).round() as usize)
            .clamp(MIN_WINDOW, MAX_WINDOW);
        let omega = std::f64::consts::TAU * frequency / sample_rate;

        bins.push(BinSpec {
            frequency,
            window,
            coeff: (2.0 * omega.cos()) as f32,
            scale: 2.0 / window as f32,
        });
    }
}

/// A magnitude as a 0..1 level, measured in dB below `reference`.
///
/// 1.0 at the reference, falling to 0 at `DISPLAY_RANGE_DB` beneath it.
fn decibels_below(magnitude: f32, reference: f32) -> f32 {
    let db = 20.0 * magnitude.max(DB_EPSILON).log10();
    let reference_db = 20.0 * reference.max(DB_EPSILON).log10();

    (1.0 + (db - reference_db) / DISPLAY_RANGE_DB).clamp(0.0, 1.0)
}

/// Generalised Goertzel magnitude for one bin over its own window length.
fn goertzel(ring: &SampleRing, hann: &[f32], bin: &BinSpec) -> f32 {
    goertzel_ending(ring, hann, bin, 0)
}

/// The same, over a window that ended `ago` samples before the newest sample.
fn goertzel_ending(ring: &SampleRing, hann: &[f32], bin: &BinSpec, ago: usize) -> f32 {
    let mut s1 = 0.0f32;
    let mut s2 = 0.0f32;
    // The window table is stored at MAX_WINDOW resolution and resampled, so
    // every bin shares one table regardless of its own window length.
    let stride = MAX_WINDOW as f32 / bin.window as f32;

    for index in 0..bin.window {
        let table_index = ((index as f32 * stride) as usize).min(MAX_WINDOW - 1);
        let sample = ring.get_ending(ago, bin.window, index) * hann[table_index];
        let s0 = sample + bin.coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }

    let power = s1 * s1 + s2 * s2 - bin.coeff * s1 * s2;
    power.max(0.0).sqrt() * bin.scale
}
