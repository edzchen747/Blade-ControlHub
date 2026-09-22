/// Runs the real detection pipeline against live loopback audio and reports what
/// it saw, without touching the keyboard.
///
/// Synthetic sine tests cannot represent a real mix: they have no harmonics, no
/// percussion and no simultaneous instruments, so they systematically flatter
/// anything that depends on how the bands compare to each other. This runs the
/// production `NoteBank` and `EqualizerMatrix` over whatever is actually playing
/// and prints the distributions, so the tuning can be judged against real music.
///
/// With `dump`, it also writes every tick's raw per-band magnitudes to a CSV, so
/// the kicks can be located from the signal itself rather than taken on trust
/// from a detector that may be mistuned.
pub fn probe_live_audio(seconds: u64, warmup: u64, dump: Option<&str>) -> Result<(), String> {
    ensure_process_mta();

    let mut capture = LoopbackCapture::open()?;
    let mut matrix = EqualizerMatrix::new(MATRIX_COLUMNS);
    let mut bank = NoteBank::new(capture.sample_rate() as f64, matrix.bands);

    println!();
    println!("Listening on the default render endpoint");
    println!(
        "  endpoint         {}",
        default_endpoint_id(AudioType::Speakers).unwrap_or_else(|| "<unreadable>".to_string())
    );
    if let Some(info) = endpoint_info(AudioType::Speakers) {
        println!("  name             {}", info.name);
        println!("  bus              {}", info.bus);
        println!("  bluetooth        {}", info.is_bluetooth());
        println!("  stream latency   {}", millis(info.stream_latency_ms));
        println!("  device period    {}", millis(info.device_period_ms));
    }
    println!("  sample rate      {} Hz", capture.sample_rate());
    println!("  bands            {}", matrix.bands);
    println!(
        "  bass band        {:.0}-{:.0} Hz",
        BASS_BAND_MIN_HZ, BASS_BAND_MAX_HZ
    );
    println!("  warmup           {warmup}s");
    println!("  window           {seconds}s");
    println!();

    let mut rows: Vec<String> = Vec::new();
    if dump.is_some() {
        let mut header = String::from("t");
        for band in 0..matrix.bands {
            header.push_str(&format!(",power{band}"));
        }
        for band in 0..matrix.bands {
            header.push_str(&format!(",level{band}"));
        }
        for band in 0..bank.onset_band_count() {
            header.push_str(&format!(",flux{band},fstr{band},energy{band}"));
        }
        header.push_str(",odf,strength,selected,fallback,hit");
        rows.push(header);
    }

    let mut samples = Samples::default();
    let warmup_end = Instant::now() + Duration::from_secs(warmup);
    let mut last = Instant::now();

    let mut last_endpoint_check = Instant::now();
    let endpoint_watcher = DefaultEndpointWatcher::new(AudioType::Speakers).ok();

    loop {
        let start = Instant::now();

        let endpoint_may_have_moved = match &endpoint_watcher {
            Some(watcher) => watcher.take_change(),
            None => start.duration_since(last_endpoint_check) >= ENDPOINT_CHECK_INTERVAL,
        };
        if endpoint_may_have_moved {
            last_endpoint_check = start;
            follow_default_endpoint(&mut capture, &mut bank, matrix.bands);
        }

        capture.pump(bank.samples_mut())?;

        let now = Instant::now();
        let dt = (now - last).as_secs_f32().clamp(0.0, 0.25);
        last = now;

        bank.analyze(dt);
        matrix.update(bank.levels(), bank.raw_levels(), bank.bass_hit(), dt);

        if now >= warmup_end {
            if dump.is_some() {
                rows.push(dump_row(&bank, (now - warmup_end).as_secs_f64()));
            }
            samples.record(&bank, &matrix, now);
            if samples.elapsed() >= Duration::from_secs(seconds) {
                break;
            }
        }

        sleep_until_next_frame(start);
    }

    samples.report();

    if let Some(path) = dump {
        std::fs::write(path, rows.join("\n"))
            .map_err(|error| format!("could not write {path}: {error}"))?;
        println!("Raw per-band magnitudes written to {path}");
        println!();
    }

    Ok(())
}

/// A timing the endpoint may simply refuse to report.
fn millis(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{value:.1} ms"),
        None => "unavailable".to_string(),
    }
}

fn dump_row(bank: &NoteBank, seconds: f64) -> String {
    let mut row = format!("{seconds:.4}");
    for value in bank.power() {
        row.push_str(&format!(",{value:.6}"));
    }
    for value in bank.levels() {
        row.push_str(&format!(",{value:.4}"));
    }
    for band in 0..bank.onset_band_count() {
        row.push_str(&format!(
            ",{:.6},{:.4},{:.6}",
            bank.onset_band_flux(band),
            bank.onset_band_strength(band),
            bank.onset_band_energy(band)
        ));
    }
    row.push_str(&format!(",{:.6},{:.4}", bank.onset_odf(), bank.onset_strength()));
    row.push_str(&format!(",{}", bank.onset_selected_band()));
    row.push_str(if bank.onset_in_fallback() { ",1" } else { ",0" });
    row.push_str(if bank.bass_hit() { ",1" } else { ",0" });
    row
}

/// Everything one run observed, one entry per analysis tick.
#[derive(Default)]
struct Samples {
    started: Option<Instant>,
    finished: Option<Instant>,
    bass_level: Vec<f32>,
    loudest_other_level: Vec<f32>,
    bar_fill: Vec<f32>,
    bass_rows: Vec<usize>,
    dominance: Vec<f32>,
    strength: Vec<f32>,
    hits: Vec<Instant>,
    /// Which band held the floor on each tick, and each band's energy, so the
    /// selection can be judged rather than taken on trust.
    selected: Vec<usize>,
    band_energy: Vec<Vec<f32>>,
    /// Ticks spent with the gap fallback open, and hits emitted while it was.
    fallback_ticks: usize,
    fallback_hits: usize,
}

impl Samples {
    fn elapsed(&self) -> Duration {
        match (self.started, self.finished) {
            (Some(start), Some(end)) => end - start,
            _ => Duration::ZERO,
        }
    }

    fn record(&mut self, bank: &NoteBank, matrix: &EqualizerMatrix, now: Instant) {
        self.started.get_or_insert(now);
        self.finished = Some(now);

        let levels = bank.levels();
        let raw = bank.raw_levels();
        let power = bank.power();

        self.bass_level.push(levels[0]);
        self.loudest_other_level
            .push(raw.iter().skip(1).copied().fold(0.0f32, f32::max));
        self.bar_fill.push(matrix.bar_fill);

        let (top, bottom) = matrix.band_rows(levels, 0);
        self.bass_rows.push(top.max(bottom));

        let loudest_other_power = power.iter().skip(1).copied().fold(0.0f32, f32::max);
        self.dominance.push(power[0] / loudest_other_power.max(1e-6));
        self.strength.push(bank.onset_strength());

        self.selected.push(bank.onset_selected_band());
        if self.band_energy.len() < bank.onset_band_count() {
            self.band_energy.resize(bank.onset_band_count(), Vec::new());
        }
        for (band, series) in self.band_energy.iter_mut().enumerate() {
            series.push(bank.onset_band_energy(band));
        }

        if bank.onset_in_fallback() {
            self.fallback_ticks += 1;
        }

        if bank.bass_hit() {
            self.hits.push(now);
            if bank.onset_in_fallback() {
                self.fallback_hits += 1;
            }
        }
    }

    fn report(&self) {
        let wall = self.elapsed().as_secs_f64();
        let ticks = self.bass_level.len();
        if ticks == 0 {
            println!("No samples captured.");
            return;
        }

        println!();
        println!("Measured over {wall:.1}s, {ticks} ticks");
        println!();

        println!("Kick detection");
        println!("  hits             {}", self.hits.len());
        println!(
            "  rate             {:.2}/s  ({:.0} bpm if every hit is a beat)",
            self.hits.len() as f64 / wall,
            self.hits.len() as f64 / wall * 60.0
        );
        if self.hits.len() > 1 {
            let mut gaps: Vec<f64> = self
                .hits
                .windows(2)
                .map(|pair| (pair[1] - pair[0]).as_secs_f64())
                .collect();
            gaps.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
            println!(
                "  gap p50          {:.3}s   (p05 {:.3}s, p95 {:.3}s)",
                percentile_f64(&gaps, 0.50),
                percentile_f64(&gaps, 0.05),
                percentile_f64(&gaps, 0.95)
            );
        }
        println!();

        self.report_selection();

        distribution(
            "Bass band level (drives the height multiplier)",
            &self.bass_level,
        );
        distribution(
            "Loudest non-bass level (the bar's second term)",
            &self.loudest_other_level,
        );
        distribution("Bar fill (row 6)", &self.bar_fill);
        distribution(
            "Onset strength, deviations above the median (fires above the threshold)",
            &self.strength,
        );
        distribution(
            "Bass dominance, raw power (must clear the hit threshold)",
            &self.dominance,
        );

        println!("Bass band height, rows lit per quadrant");
        let total = self.bass_rows.len() as f64;
        for rows in 0..=QUADRANT_ROWS {
            let count = self.bass_rows.iter().filter(|value| **value == rows).count();
            let share = count as f64 / total * 100.0;
            println!("  {rows} row(s)        {share:>6.1}%  {}", bar_glyph(share));
        }
        println!();

        let near_max = share_at_least(&self.bar_fill, 0.9);
        let topped = self
            .bass_rows
            .iter()
            .filter(|value| **value >= QUADRANT_ROWS)
            .count() as f64
            / total
            * 100.0;

        println!("Summary");
        println!("  bar at 90%+      {near_max:>6.1}% of the time");
        println!("  bass band full   {topped:>6.1}% of the time");
        println!();
    }
}

impl Samples {
    /// Which band the detector settled on, and how hard it had to lean on the
    /// fallback to keep the beat going.
    ///
    /// A band that holds the floor for nearly the whole run is the healthy
    /// case. Repeated toggling means `BAND_PRESENCE_ENGAGE` and `RELEASE` sit
    /// too close together; a high fallback share means selection has picked a
    /// band the beat does not actually live in.
    fn report_selection(&self) {
        let ticks = self.selected.len();
        if ticks == 0 {
            return;
        }

        println!("Band selection");
        for band in 0..self.band_energy.len() {
            let held = self.selected.iter().filter(|value| **value == band).count();
            let share = held as f64 / ticks as f64 * 100.0;
            let energy = &self.band_energy[band];
            let mean = energy.iter().sum::<f32>() / energy.len().max(1) as f32;
            let label = match band {
                0 => "sub  20-60Hz",
                1 => "punch 60-150Hz",
                _ => "band",
            };
            println!(
                "  {label:<15} held {share:>6.1}%   mean energy {mean:.3}  {}",
                bar_glyph(share)
            );
        }

        let switches = self
            .selected
            .windows(2)
            .filter(|pair| pair[0] != pair[1])
            .count();
        println!("  switches         {switches}");
        println!(
            "  fallback open    {:>6.1}% of ticks, {} of {} hits",
            self.fallback_ticks as f64 / ticks as f64 * 100.0,
            self.fallback_hits,
            self.hits.len()
        );
        println!();
    }
}

fn share_at_least(values: &[f32], threshold: f32) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().filter(|value| **value >= threshold).count() as f64 / values.len() as f64 * 100.0
}

fn distribution(label: &str, values: &[f32]) {
    if values.is_empty() {
        return;
    }
    let mut sorted: Vec<f32> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    let mean = sorted.iter().sum::<f32>() / sorted.len() as f32;

    println!("{label}");
    println!(
        "  mean {mean:.3}   p05 {:.3}   p50 {:.3}   p95 {:.3}   max {:.3}",
        percentile_f32(&sorted, 0.05),
        percentile_f32(&sorted, 0.50),
        percentile_f32(&sorted, 0.95),
        sorted[sorted.len() - 1]
    );
    println!();
}

fn percentile_f32(sorted: &[f32], fraction: f32) -> f32 {
    let rank = (fraction * (sorted.len() - 1) as f32).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn percentile_f64(sorted: &[f64], fraction: f64) -> f64 {
    let rank = (fraction * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn bar_glyph(share: f64) -> String {
    "#".repeat(((share / 2.0).round() as usize).min(50))
}
