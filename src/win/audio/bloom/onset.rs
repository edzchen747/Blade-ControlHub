/// Spectral-flux onset detection for kick drums.
///
/// Separate from the display bank because the two want opposite things. The
/// display bank gives each bin its own window length - four periods of its own
/// frequency - which suits a spectrum display but makes flux meaningless: the
/// 40 Hz bin smears a transient across 85 ms while a treble bin resolves it in
/// 1.3 ms, so differencing them compares incompatible time scales. Every bin
/// here shares one window.
///
/// It also runs on **its own clock**, stepping through the sample stream in
/// fixed hops rather than once per rendered frame. Tied to the frame rate the
/// flux was sampled every 34 ms with 21% window overlap, against a kick attack
/// of 5-10 ms; no threshold can make detection consistent when the measurement
/// itself smears the transient it is looking for.
///
/// The detection function is textbook spectral flux:
///
/// * **Log-compressed magnitudes.** A linear difference scales with absolute
///   loudness, so a loud passage produces large flux whether or not anything
///   changed. Compression makes it a measure of *relative* change.
/// * **Per-bin differences, summed.** Differencing a per-band maximum instead
///   reports nothing when three bins jump but the loudest holds steady.
/// * **Split low bands.** Sub-bass and kick punch are measured as two
///   independent detectors rather than one wide one, because a band spanning
///   both dilutes whichever of them moved into the other's noise. Either
///   firing is enough.
/// * **Median/MAD normalisation**, which makes each band's output unitless and
///   lets a fixed threshold hold across quiet and loud passages alike.
struct OnsetBank {
    /// Sub-bass, and the kick's punch range. Independent detectors.
    sub: FluxBand,
    kick: FluxBand,
    /// The mids, used only as the reference for the vocal rejection test, so it
    /// needs no history or peak-picking of its own.
    mid: Vec<BinSpec>,
    previous_mid: Vec<f32>,
    /// Sample count already analysed, so hops land on the stream rather than on
    /// whenever a frame happened to be drawn.
    processed: u64,
    /// Hops remaining before another hit can register. Shared between the bands:
    /// one strike excites both, and it should report once.
    refractory: usize,
    /// Latched for the frame: a hit anywhere in this frame's hops counts.
    hit: bool,
    odf: f32,
    strength: f32,
    history_len: usize,
}

impl OnsetBank {
    fn new(sample_rate: f64) -> Self {
        let hop_seconds = FLUX_HOP_SAMPLES as f32 / sample_rate as f32;
        let history_len = ((FLUX_NORMALISE_SECONDS / hop_seconds).round() as usize).max(8);

        let mid = fixed_window_bins(
            sample_rate,
            FLUX_MID_MIN_HZ,
            FLUX_MID_MAX_HZ,
            FLUX_MID_BINS,
        );

        Self {
            sub: FluxBand::new(
                sample_rate,
                FLUX_SUB_MIN_HZ,
                FLUX_SUB_MAX_HZ,
                FLUX_SUB_BINS,
                history_len,
            ),
            kick: FluxBand::new(
                sample_rate,
                FLUX_KICK_MIN_HZ,
                FLUX_KICK_MAX_HZ,
                FLUX_KICK_BINS,
                history_len,
            ),
            previous_mid: vec![0.0; mid.len()],
            mid,
            processed: 0,
            refractory: 0,
            hit: false,
            odf: 0.0,
            strength: 0.0,
            history_len,
        }
    }

    fn hit(&self) -> bool {
        self.hit
    }

    /// The raw flux of whichever low band is currently driving detection.
    fn odf(&self) -> f32 {
        self.odf
    }

    /// The stronger of the two bands' normalised strengths.
    fn strength(&self) -> f32 {
        self.strength
    }

    /// Runs every whole hop of audio that has arrived since the last call.
    ///
    /// Usually two or three at 30 fps. A hit in any of them latches for the
    /// frame, so the visuals still see one flag per frame while the detection
    /// itself runs at its own, much finer rate.
    fn analyze(&mut self, ring: &SampleRing, hann: &[f32], sample_rate: f64) {
        self.hit = false;

        let available = ring.written();
        if self.processed == 0 {
            // Wait for a full window before the first measurement, or the first
            // flux would be against an empty buffer.
            self.processed = available.max(FLUX_WINDOW_SAMPLES as u64);
        }

        let hop = FLUX_HOP_SAMPLES as u64;
        let mut pending = available.saturating_sub(self.processed) / hop;

        // The ring only holds `MAX_WINDOW`, so anything older than that is gone.
        // After a stall, skip the backlog rather than analysing stale audio.
        let reachable = ((MAX_WINDOW - FLUX_WINDOW_SAMPLES) / FLUX_HOP_SAMPLES) as u64;
        if pending > reachable {
            self.processed += (pending - reachable) * hop;
            pending = reachable;
        }

        for step in (0..pending).rev() {
            // How far back this hop's window ends, counted from the newest
            // sample: the oldest pending hop is the furthest back.
            let ago = (step * hop + (available - self.processed - pending * hop)) as usize;
            self.step(ring, hann, ago, sample_rate);
        }
        self.processed += pending * hop;
    }

    fn step(&mut self, ring: &SampleRing, hann: &[f32], ago: usize, _sample_rate: f64) {
        self.refractory = self.refractory.saturating_sub(1);

        // A voice puts its harmonics in the mids, so a syllable that looks like
        // an onset down low shows a matching jump up there. A kick's harmonics
        // stay inside the low range. Compared per bin, since the three bands
        // hold different numbers of them.
        let mid_flux = band_flux(ring, hann, ago, &self.mid, &mut self.previous_mid);
        let mid_per_bin = mid_flux / self.mid.len().max(1) as f32;

        self.sub.measure(ring, hann, ago, mid_per_bin);
        self.kick.measure(ring, hann, ago, mid_per_bin);

        // Report whichever band is shouting louder, for the bench.
        if self.sub.strength >= self.kick.strength {
            self.odf = self.sub.flux;
            self.strength = self.sub.strength;
        } else {
            self.odf = self.kick.flux;
            self.strength = self.kick.strength;
        }

        // Either band peaking is a kick. They are deliberately not summed: a sub
        // drop with a flat punch range, and a punchy kick with nothing under it,
        // are both real, and each would be averaged away by the other.
        let peaked = self.sub.peaked() || self.kick.peaked();
        if self.refractory == 0 && peaked {
            self.hit = true;
            self.refractory = self.refractory_hops();
        }

        self.sub.advance();
        self.kick.advance();
    }

    fn refractory_hops(&self) -> usize {
        ((BASS_REFRACTORY_SECONDS * self.history_len as f32) / FLUX_NORMALISE_SECONDS).round()
            as usize
    }
}

/// One low band's detector: its bins, its own flux history, its own threshold.
///
/// Holding the history per band is the point of the split. A median taken across
/// sub and punch together is dominated by whichever of them is busier, so the
/// quieter one never clears a threshold measured largely against the other.
struct FluxBand {
    bins: Vec<BinSpec>,
    previous: Vec<f32>,
    /// Recent flux, for this band's own median and deviation.
    history: VecDeque<f32>,
    history_len: usize,
    /// The previous two strengths, so an onset is reported at its peak.
    previous_strength: f32,
    strength_before: f32,
    /// Whether the hop behind the one just measured cleared the threshold, and
    /// the pending value for the hop just measured.
    previous_passed: bool,
    passed: bool,
    flux: f32,
    strength: f32,
}

impl FluxBand {
    fn new(sample_rate: f64, start: f64, end: f64, count: usize, history_len: usize) -> Self {
        let bins = fixed_window_bins(sample_rate, start, end, count);
        Self {
            previous: vec![0.0; bins.len()],
            bins,
            history: VecDeque::new(),
            history_len,
            previous_strength: 0.0,
            strength_before: 0.0,
            previous_passed: false,
            passed: false,
            flux: 0.0,
            strength: 0.0,
        }
    }

    /// Measures this hop and folds it into the normalised detection function.
    fn measure(&mut self, ring: &SampleRing, hann: &[f32], ago: usize, mid_per_bin: f32) {
        let flux = band_flux(ring, hann, ago, &self.bins, &mut self.previous);
        self.flux = flux;

        self.history.push_back(flux);
        while self.history.len() > self.history_len {
            self.history.pop_front();
        }

        // The adaptive threshold: a level measured against this band's own recent
        // past, so it rides up through a chorus and back down through a verse
        // rather than being a figure fixed in advance. Medians rather than means
        // so the onsets being looked for do not raise the bar against themselves.
        let (median, deviation) = median_and_deviation(&self.history);
        self.strength = (flux - median) / (deviation + FLUX_DEVIATION_FLOOR);

        let per_bin = flux / self.bins.len().max(1) as f32;
        self.passed =
            self.strength >= FLUX_THRESHOLD && per_bin >= mid_per_bin * FLUX_DOMINANCE;
    }

    /// Whether the *previous* hop was a peak that cleared the threshold.
    ///
    /// Reported at the peak rather than at the first hop over the line: a rising
    /// edge clears it on several consecutive hops, so firing on the first would
    /// report one kick repeatedly.
    fn peaked(&self) -> bool {
        self.previous_strength >= self.strength_before
            && self.previous_strength > self.strength
            && self.previous_passed
    }

    /// Rolls the one-hop lookahead forward. Called after `peaked`.
    fn advance(&mut self) {
        self.strength_before = self.previous_strength;
        self.previous_strength = self.strength;
        self.previous_passed = self.passed;
    }
}

/// Half-wave-rectified difference of log magnitudes, summed over the bins.
///
/// `previous` holds the last hop's log magnitudes and is updated in place.
fn band_flux(
    ring: &SampleRing,
    hann: &[f32],
    ago: usize,
    bins: &[BinSpec],
    previous: &mut [f32],
) -> f32 {
    let mut flux = 0.0f32;
    for (index, bin) in bins.iter().enumerate() {
        let compressed = compress(goertzel_ending(ring, hann, bin, ago));
        flux += (compressed - previous[index]).max(0.0);
        previous[index] = compressed;
    }
    flux
}

/// Log compression, the heart of the change.
///
/// `ln(1 + k*x)` rather than a bare logarithm so it is defined at zero and stays
/// monotonic all the way down, with no floor to pick.
fn compress(magnitude: f32) -> f32 {
    (1.0 + FLUX_COMPRESSION * magnitude.max(0.0)).ln()
}

/// Bins log-spaced across a range, every one sharing `FLUX_WINDOW_SAMPLES`.
///
/// The fixed window is the point: flux is only meaningful between bins measured
/// over the same span of time.
fn fixed_window_bins(sample_rate: f64, start: f64, end: f64, count: usize) -> Vec<BinSpec> {
    let window = FLUX_WINDOW_SAMPLES.min(MAX_WINDOW);
    let ratio = (end / start).powf(1.0 / count as f64);

    (0..count)
        .map(|index| {
            let frequency = start * ratio.powi(index as i32);
            let omega = std::f64::consts::TAU * frequency / sample_rate;
            BinSpec {
                frequency,
                window,
                coeff: (2.0 * omega.cos()) as f32,
                scale: 2.0 / window as f32,
            }
        })
        .collect()
}

/// The median of the window and the median absolute deviation from it.
///
/// Medians rather than means throughout: a single loud transient would drag a
/// mean up behind itself and mask the next one.
fn median_and_deviation(history: &VecDeque<f32>) -> (f32, f32) {
    if history.is_empty() {
        return (0.0, 0.0);
    }

    let mut values: Vec<f32> = history.iter().copied().collect();
    values.sort_by(|a, b| a.total_cmp(b));
    let median = values[values.len() / 2];

    let mut deviations: Vec<f32> = values.iter().map(|value| (value - median).abs()).collect();
    deviations.sort_by(|a, b| a.total_cmp(b));

    (median, deviations[deviations.len() / 2])
}
