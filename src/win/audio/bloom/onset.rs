/// Spectral-flux onset detection for kick drums.
///
/// Separate from the display bank because the two want opposite things. The
/// display bank gives each bin its own window length — four periods of its own
/// frequency — which suits a spectrum display but makes flux meaningless: the
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
/// * **Median/MAD normalisation**, which makes the output unitless and lets a
///   fixed threshold hold across quiet and loud passages alike.
struct OnsetBank {
    low: Vec<BinSpec>,
    mid: Vec<BinSpec>,
    previous_low: Vec<f32>,
    previous_mid: Vec<f32>,
    /// Recent flux, for the median and deviation.
    history: VecDeque<f32>,
    history_len: usize,
    /// Sample count already analysed, so hops land on the stream rather than on
    /// whenever a frame happened to be drawn.
    processed: u64,
    /// Hops remaining before another hit can register.
    refractory: usize,
    /// The previous two strengths, so an onset is reported at its peak.
    previous_strength: f32,
    strength_before: f32,
    previous_passed: bool,
    /// Latched for the frame: a hit anywhere in this frame's hops counts.
    hit: bool,
    odf: f32,
    strength: f32,
}

impl OnsetBank {
    fn new(sample_rate: f64) -> Self {
        let low = fixed_window_bins(sample_rate, FLUX_MIN_HZ, FLUX_MAX_HZ, FLUX_BINS);
        let mid = fixed_window_bins(
            sample_rate,
            FLUX_MID_MIN_HZ,
            FLUX_MID_MAX_HZ,
            FLUX_MID_BINS,
        );
        let hop_seconds = FLUX_HOP_SAMPLES as f32 / sample_rate as f32;

        Self {
            previous_low: vec![0.0; low.len()],
            previous_mid: vec![0.0; mid.len()],
            low,
            mid,
            history: VecDeque::new(),
            history_len: ((FLUX_NORMALISE_SECONDS / hop_seconds).round() as usize).max(8),
            processed: 0,
            refractory: 0,
            previous_strength: 0.0,
            strength_before: 0.0,
            previous_passed: false,
            hit: false,
            odf: 0.0,
            strength: 0.0,
        }
    }

    fn hit(&self) -> bool {
        self.hit
    }

    fn odf(&self) -> f32 {
        self.odf
    }

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

        let low_flux = band_flux(ring, hann, ago, &self.low, &mut self.previous_low);
        let mid_flux = band_flux(ring, hann, ago, &self.mid, &mut self.previous_mid);
        self.odf = low_flux;

        self.history.push_back(low_flux);
        while self.history.len() > self.history_len {
            self.history.pop_front();
        }

        let (median, deviation) = median_and_deviation(&self.history);
        self.strength = (low_flux - median) / (deviation + FLUX_DEVIATION_FLOOR);

        // A voice puts its harmonics in the mids, so a syllable that looks like
        // an onset down here shows a matching jump up there. A kick's harmonics
        // stay inside the low range.
        let passed = self.strength >= FLUX_THRESHOLD && low_flux >= mid_flux * FLUX_DOMINANCE;

        // Report at the peak rather than the first hop over the threshold: a
        // rising edge clears it on several consecutive hops, so firing on the
        // first reports one kick repeatedly.
        let is_peak = self.previous_strength >= self.strength_before
            && self.previous_strength > self.strength;
        if self.refractory == 0 && is_peak && self.previous_passed {
            self.hit = true;
            self.refractory = self.refractory_hops();
        }

        self.strength_before = self.previous_strength;
        self.previous_strength = self.strength;
        self.previous_passed = passed;
    }

    fn refractory_hops(&self) -> usize {
        ((BASS_REFRACTORY_SECONDS * self.history_len as f32) / FLUX_NORMALISE_SECONDS).round()
            as usize
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
