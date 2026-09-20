/// Which half of the mirror gets a band's odd half-row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HalfSide {
    Top,
    Bottom,
}

/// The mirrored four-quadrant equalizer, plus the bass bar on the last row.
///
/// Rows 0-5 are mirrored about a horizontal axis between rows 2 and 3, and about
/// a vertical axis down the middle of the keyboard, giving four quadrants of
/// three rows each. The top-right quadrant is the reference: low notes at its
/// inner edge (the keyboard centre) running out to high notes at the edge. The
/// other three are reflections, so bass sits in the middle of the board and
/// treble at both outer edges.
///
/// Three rows per quadrant would normally give four levels (0-3). Mirroring buys
/// half-steps: a band can light `n` rows on one side and `n+1` on the other, and
/// the eye reads the pair as `n.5`. That takes it to seven levels.
struct EqualizerMatrix {
    columns: usize,
    bands: usize,
    /// The side currently showing a band's half-row, latched while it lasts.
    half_sides: Vec<Option<HalfSide>>,
    /// The single colour everything is drawn in, cycling the hue circle.
    hue: f32,
    /// The bass bar's own envelope, 0..1 of the row.
    bar_fill: f32,
    /// How long the input has been silent, against `BAR_SILENCE_DELAY_SECONDS`.
    /// A short gap is not yet silence.
    silence_elapsed: f32,
    /// Seconds left on the current bass boost.
    boost_remaining: f32,
    /// Seconds left on the white flash that marks a boost firing.
    flash_remaining: f32,
    /// Per block column: the key nominated to drop, and the roll it has to beat.
    /// Redrawn every `BLOCK_DITHER_SECONDS`.
    dither: [(f32, usize); CENTRE_BLOCK_WIDTH],
    dither_elapsed: f32,
    /// The one-shot boost envelope, 1.0 as it fires and fading to 0.
    boost_envelope: f32,
    /// The bass level the height boost and skirt actually see: the live level
    /// while a boost is running, nothing once it has expired.
    boost_bass: f32,
    rng: u32,
}

impl EqualizerMatrix {
    fn new(columns: usize) -> Self {
        let columns = columns.max(1);
        let bands = band_count(columns);
        Self {
            columns,
            bands,
            half_sides: vec![None; bands],
            hue: 0.0,
            bar_fill: 0.0,
            silence_elapsed: BAR_SILENCE_DELAY_SECONDS,
            dither: [(1.0, 0); CENTRE_BLOCK_WIDTH],
            dither_elapsed: BLOCK_DITHER_SECONDS,
            boost_remaining: 0.0,
            flash_remaining: 0.0,
            boost_envelope: 0.0,
            boost_bass: 0.0,
            rng: 0x9E37_79B9,
        }
    }

    /// xorshift32. Only used to pick a side, so scatter is all that is needed.
    fn next_random(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    /// `levels` drives the quadrants; `raw` is the same measurement before
    /// smoothing, which the bar uses so it can fall on its own schedule.
    fn update(&mut self, levels: &[f32], raw: &[f32], bass_hit: bool, dt: f32) {
        self.hue = (self.hue + dt / RAINBOW_PERIOD_SECONDS).fract();

        // Before anything reads a band height: the half-row latch below calls
        // `band_height`, which works off the gated bass rather than the raw one.
        //
        // Hit detection lives in the note bank, which holds the raw magnitudes
        // that tell a kick from a vocal transient.
        self.update_boost(
            levels.first().copied().unwrap_or(0.0),
            raw.first().copied().unwrap_or(0.0),
            bass_hit,
            dt,
        );

        self.update_dither(dt);

        self.update_silence(raw, dt);

        let target = bar_target(raw, self.boost_envelope, self.silence_settled());
        let rate = if target >= self.bar_fill {
            BAR_ATTACK
        } else {
            BAR_RELEASE
        };
        let step = (rate * dt / REFERENCE_TICK).clamp(0.0, 1.0);
        self.bar_fill += (target - self.bar_fill) * step;

        // Latch the half-row side on the edge where it appears, and hold it
        // until the band drops back below the half. Re-rolling every frame would
        // strobe one key at the frame rate.
        for band in 0..self.bands {
            let height = self.band_height(levels, band);
            let wants_half = height - height.floor() >= 0.5;

            if !wants_half {
                self.half_sides[band] = None;
            } else if self.half_sides[band].is_none() {
                self.half_sides[band] = Some(if self.next_random() & 1 == 0 {
                    HalfSide::Top
                } else {
                    HalfSide::Bottom
                });
            }
        }
    }

    /// Advances the one-shot boost envelope.
    ///
    /// The boost restarts on every kick-drum hit that carries enough weight, and
    /// runs for `BASS_BOOST_SECONDS`, fading as it goes.
    ///
    /// `raw_bass` is the unsmoothed band level, used for the loudness gate so a
    /// sharp kick is judged on its own height rather than on an envelope still
    /// climbing towards it.
    fn update_boost(&mut self, bass: f32, raw_bass: f32, bass_hit: bool, dt: f32) {
        // A kick still counts as a kick; it just does not earn the boost unless
        // there is real level behind it. See `BASS_BOOST_MIN_LEVEL`.
        if bass_hit && raw_bass >= BASS_BOOST_MIN_LEVEL {
            self.boost_remaining = BASS_BOOST_SECONDS;
            self.flash_remaining = BAND_FLASH_SECONDS;
        }

        self.boost_remaining = (self.boost_remaining - dt).max(0.0);
        self.flash_remaining = (self.flash_remaining - dt).max(0.0);

        // Fading rather than cutting out: a hard edge at the end of the window
        // would snap every band down a row at once.
        self.boost_envelope = self.boost_remaining / BASS_BOOST_SECONDS;
        self.boost_bass = bass * self.boost_envelope;
    }

    /// How many rows tall a band stands in each quadrant, 0..=`QUADRANT_ROWS`.
    ///
    /// The bass band multiplies every band including itself. With no bass the
    /// multiplier is 1 and nothing can exceed a single row, however loud it is.
    /// At full bass the centre band is tripled and fills its quadrant, tapering
    /// to double at the outermost band.
    ///
    /// The bass also puts a floor under the bands above it, so a hit spreads
    /// outward instead of spiking the centre columns alone.
    fn band_height(&self, levels: &[f32], band: usize) -> f32 {
        // The gated bass, so a held note boosts once and then lets go.
        let bass = self.boost_bass;
        let multiplier = 1.0 + bass * bass_boost(band, self.bands);
        let level = levels
            .get(band)
            .copied()
            .unwrap_or(0.0)
            .max(band_floor(band, self.bands, bass));
        (level * multiplier).clamp(0.0, QUADRANT_ROWS as f32)
    }

    /// Rows lit on each side of the mirror for a band.
    fn band_rows(&self, levels: &[f32], band: usize) -> (usize, usize) {
        let height = self.band_height(levels, band);
        let full = height.floor() as usize;
        let half = height - height.floor() >= 0.5;

        let extra = |side: HalfSide| {
            usize::from(half && self.half_sides.get(band).copied().flatten() == Some(side))
        };

        (
            (full + extra(HalfSide::Top)).min(QUADRANT_ROWS),
            (full + extra(HalfSide::Bottom)).min(QUADRANT_ROWS),
        )
    }

    fn render(&self, levels: &[f32]) -> Vec<Vec<ThemeColor>> {
        // Resolved once per frame rather than once per key.
        let band_rows: Vec<(usize, usize)> = (0..self.bands)
            .map(|band| self.band_rows(levels, band))
            .collect();
        let bar = self.bar_columns();
        let bass = levels.first().copied().unwrap_or(0.0);

        // Brightness first, colour last: the bloom pass has to see how bright
        // each key wants to be before anything is turned into a colour.
        let mut values: Vec<Vec<f32>> = (0..MATRIX_ROWS)
            .map(|row| {
                (0..self.columns)
                    .map(|column| self.key_value(row, column, &band_rows, bar))
                    .collect()
            })
            .collect();

        self.apply_bloom(&mut values, bass);

        // Washing the colour out rather than replacing it: `value` still
        // carries each key's brightness, so an unlit key stays unlit through
        // the flash instead of the whole board turning on.
        let flash = self.flash_envelope();
        // Keeps the dark end of the hue circle out of the cycle. The bar takes
        // this too - it is the same colour, just not the flash.
        let base = saturation_for_hue(self.hue);

        values
            .iter()
            .enumerate()
            .map(|(row, columns)| {
                let saturation = if row == BAR_ROW {
                    base
                } else {
                    base.min(1.0 - flash)
                };
                columns
                    .iter()
                    .map(|value| hsv_to_rgb(self.hue, saturation, *value).to_theme_color())
                    .collect()
            })
            .collect()
    }

    /// How bright a key wants to be before any bloom spills onto it.
    fn key_value(
        &self,
        row: usize,
        column: usize,
        band_rows: &[(usize, usize)],
        bar: usize,
    ) -> f32 {
        if row == BAR_ROW {
            return if column < bar { 1.0 } else { 0.0 };
        }
        if self.in_centre_block(row, column) {
            // The one non-binary element: the block tracks the bar's envelope,
            // stretched so it spans the full brightness range.
            let brightness = centre_block_brightness(self.bar_fill);
            if self.dithered_out(row, column, brightness) {
                return 0.0;
            }
            return brightness;
        }
        if self.is_key_lit(row, column, band_rows, bar) {
            1.0
        } else {
            0.0
        }
    }

    /// Spills light from every lit key onto its immediate neighbours.
    ///
    /// The bar on `BAR_ROW` takes no part in either direction: it neither glows
    /// nor is glowed onto, so it stays a crisp readout and does not bleed into
    /// the row of bands above it.
    fn apply_bloom(&self, values: &mut [Vec<f32>], bass: f32) {
        // Spill is measured against the pre-bloom values, so the glow does not
        // feed on itself and creep outward a key at a time.
        let source: Vec<Vec<f32>> = values.to_vec();
        let (columns, bands) = (self.columns, self.bands);

        for (row, keys) in values.iter_mut().enumerate() {
            if row == BAR_ROW {
                continue;
            }
            for (column, value) in keys.iter_mut().enumerate() {
                // A key's own band decides how much it can glow, so the spill
                // fades with distance from the bass at the centre.
                let strength =
                    bloom_strength(band_for_column(column, columns), bands, bass);
                let mut brightest = 0.0f32;
                for (neighbour_row, neighbour_column) in self.neighbours(row, column) {
                    if neighbour_row == BAR_ROW {
                        continue;
                    }
                    brightest = brightest.max(source[neighbour_row][neighbour_column]);
                }
                *value = value.max(brightest * strength);
            }
        }
    }

    /// The keys immediately touching this one, clipped to the matrix.
    fn neighbours(&self, row: usize, column: usize) -> Vec<(usize, usize)> {
        const ORTHOGONAL: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        const DIAGONAL: [(i32, i32); 4] = [(-1, -1), (-1, 1), (1, -1), (1, 1)];

        ORTHOGONAL
            .iter()
            .chain(if BLOOM_DIAGONALS { &DIAGONAL[..] } else { &[] })
            .filter_map(|(row_step, column_step)| {
                let neighbour_row = row as i32 + row_step;
                let neighbour_column = column as i32 + column_step;
                if neighbour_row < 0
                    || neighbour_row >= MATRIX_ROWS as i32
                    || neighbour_column < 0
                    || neighbour_column >= self.columns as i32
                {
                    return None;
                }
                Some((neighbour_row as usize, neighbour_column as usize))
            })
            .collect()
    }

    /// Redraws which key each block column would drop, once per interval.
    fn update_dither(&mut self, dt: f32) {
        self.dither_elapsed += dt;
        if self.dither_elapsed < BLOCK_DITHER_SECONDS {
            return;
        }
        self.dither_elapsed = 0.0;

        for column in 0..CENTRE_BLOCK_WIDTH {
            // The roll a key has to beat to survive, and which of the column's
            // keys is the one at risk.
            let roll = (self.next_random() >> 8) as f32 / (1u32 << 24) as f32;
            let row = self.next_random() as usize % CENTRE_BLOCK_HEIGHT.max(1);
            self.dither[column] = (roll, row);
        }
    }

    /// Whether this block key is the one its column is dropping right now.
    fn dithered_out(&self, row: usize, column: usize, brightness: f32) -> bool {
        let probability = (1.0 - brightness).min(BLOCK_DITHER_MAX_PROBABILITY);
        let columns = self.centre_block_columns();
        let half = CENTRE_BLOCK_HEIGHT / 2;
        let block_column = column - columns.start;
        let block_row = row - (QUADRANT_ROWS - half);

        let (roll, nominated) = self.dither[block_column.min(CENTRE_BLOCK_WIDTH - 1)];
        nominated == block_row && roll < probability
    }

    /// The columns spanned by the centre block.
    ///
    /// An even keyboard width centres exactly; an odd one cannot place an
    /// even-width block symmetrically, so it lands half a column to the left.
    fn centre_block_columns(&self) -> std::ops::Range<usize> {
        let start = self.columns.saturating_sub(CENTRE_BLOCK_WIDTH) / 2;
        start..(start + CENTRE_BLOCK_WIDTH).min(self.columns)
    }

    /// Whether a key belongs to the centre block, which straddles the mirror
    /// axis with half its rows above and half below.
    fn in_centre_block(&self, row: usize, column: usize) -> bool {
        let half = CENTRE_BLOCK_HEIGHT / 2;
        let rows = (QUADRANT_ROWS - half)..(QUADRANT_ROWS + half);

        rows.contains(&row) && self.centre_block_columns().contains(&column)
    }

    fn is_key_lit(
        &self,
        row: usize,
        column: usize,
        band_rows: &[(usize, usize)],
        bar: usize,
    ) -> bool {
        if row == BAR_ROW {
            return column < bar;
        }
        if row >= VISUALIZER_ROWS {
            return false;
        }

        let (top_rows, bottom_rows) = band_rows[band_for_column(column, self.columns)];

        // Bands grow outward from the mirror axis, so depth 0 is the row against
        // the axis and the bar extends toward the outer edge.
        if row < QUADRANT_ROWS {
            let depth = QUADRANT_ROWS - 1 - row;
            depth < top_rows
        } else {
            let depth = row - QUADRANT_ROWS;
            depth < bottom_rows
        }
    }

    /// How white the bands are drawn right now, 1.0 as a boost fires.
    fn flash_envelope(&self) -> f32 {
        (self.flash_remaining / BAND_FLASH_SECONDS).clamp(0.0, 1.0)
    }

    /// Tracks how long the input has been quiet.
    ///
    /// Any audio at all resets it, so the delay only ever measures an unbroken
    /// run of silence.
    fn update_silence(&mut self, raw: &[f32], dt: f32) {
        if is_silent(raw) {
            self.silence_elapsed += dt;
        } else {
            self.silence_elapsed = 0.0;
        }
    }

    /// Whether the quiet has lasted long enough to be treated as silence.
    fn silence_settled(&self) -> bool {
        self.silence_elapsed >= BAR_SILENCE_DELAY_SECONDS
    }

    /// How many columns of the bass bar are lit, counting from the left edge.
    ///
    /// Plain rounding, with no minimum of one key: a fill that is still decaying
    /// toward zero would otherwise hold a single key lit long after the audio
    /// stopped. `BAR_MIN_FILL` is already large enough to round up to one key on
    /// its own whenever there is something playing.
    fn bar_columns(&self) -> usize {
        ((self.bar_fill * self.columns as f32).round() as usize).min(self.columns)
    }
}

/// How far light spills onto a key in `band`, driven by the bass level.
///
/// At rest there is no glow at all and the bands stay crisp. As the bass comes
/// in the board swells outward, hardest at the centre where the bass itself is
/// drawn and tapering to `BLOOM_STRENGTH_OUTER` at the outermost band.
fn bloom_strength(band: usize, bands: usize, bass: f32) -> f32 {
    let span = bands.saturating_sub(1).max(1) as f32;
    let distance = (band as f32 / span).clamp(0.0, 1.0);
    let ceiling = BLOOM_STRENGTH + (BLOOM_STRENGTH_OUTER - BLOOM_STRENGTH) * distance;

    bass.clamp(0.0, 1.0) * ceiling
}

/// The brightness the centre block draws for a given bar fill.
///
/// A plain linear stretch: the bar's resting floor lands at effectively zero and
/// a full bar lands at full, so the block covers the whole range rather than the
/// top nine tenths of it.
fn centre_block_brightness(bar_fill: f32) -> f32 {
    (bar_fill * CENTRE_BLOCK_SCALE - CENTRE_BLOCK_OFFSET).clamp(0.0, 1.0)
}

/// How much of the row the bar wants to fill, before its envelope.
///
/// Bass alone spans `BAR_MIN_FILL` to `BAR_MIN_FILL + BAR_BASS_SPAN`; the
/// loudest non-bass band supplies the headroom above that, which is what lets
/// the bar reach the end of the row.
///
/// The floor applies only while something is playing. With no audio at all the
/// target is zero, so the bar and the centre block both go dark rather than
/// leaving a stub lit against a silent system - but only once `settled` says the
/// quiet has outlasted `BAR_SILENCE_DELAY_SECONDS`. Through a shorter gap the
/// bar holds its floor, so a momentary drop-out does not read as a flicker.
///
/// `boost` is the one-shot envelope, and it decides what the bar is *about*.
///
/// Off the boost the bass is just another band: the bar follows whichever band
/// is loudest, at the ordinary share, and sits low and mobile. A kick promotes
/// the bass, handing it the whole span with the `BAR_BASS_BOOST` lift on top, so
/// the bar leaps to near-full and then falls back as the window closes.
///
/// Giving the bass the full span at all times is what kept the bar pinned near
/// the top: its two terms were both high almost all the time, so there was
/// nowhere left for a kick to move it to.
fn bar_target(levels: &[f32], boost: f32, settled: bool) -> f32 {
    let bass = levels.first().copied().unwrap_or(0.0);
    let loudest_other = levels.iter().skip(1).copied().fold(0.0f32, f32::max);

    if is_silent(levels) {
        return if settled { 0.0 } else { BAR_MIN_FILL };
    }

    let ordinary = bass.max(loudest_other) * BAR_OTHER_SHARE;
    // Capped before it is scaled, so a boosted kick fills the row and no more.
    let promoted = (bass * (1.0 + BAR_BASS_BOOST)).min(1.0);
    let driver = ordinary + (promoted - ordinary) * boost.clamp(0.0, 1.0);

    (BAR_MIN_FILL + driver * BAR_BASS_SPAN).clamp(BAR_MIN_FILL, 1.0)
}

/// Whether nothing at all is playing.
///
/// The band gate already zeroes anything under the noise floor, so silence
/// arrives here as an exact zero rather than as something merely small.
fn is_silent(levels: &[f32]) -> bool {
    levels.iter().copied().fold(0.0f32, f32::max) <= f32::EPSILON
}

/// How much the bass can multiply a band's height, at full bass.
///
/// Tapers from `BASS_BOOST_CENTRE` at the bass band itself out to
/// `BASS_BOOST_OUTER` at the highest, so the bass drives the middle of the
/// keyboard harder than it drives the edges.
fn bass_boost(band: usize, bands: usize) -> f32 {
    let span = bands.saturating_sub(1).max(1) as f32;
    let distance = (band as f32 / span).clamp(0.0, 1.0);

    BASS_BOOST_CENTRE + (BASS_BOOST_OUTER - BASS_BOOST_CENTRE) * distance
}

/// The floor the bass band puts under the bands above it.
///
/// The band next to the bass is lifted to the bass level outright, and the lift
/// falls away linearly to nothing at the outermost band. A bass hit therefore
/// spreads as a triangular skirt from the centre of the keyboard rather than
/// spiking the middle columns on their own.
///
/// The floor is applied to the *level*, so the bass multiplier acts on it as
/// well: at full bass the neighbouring band is lifted to 1.0 and then tripled,
/// filling its quadrant. With no bass every floor is zero, which is what keeps
/// the "one row maximum without bass" rule intact.
fn band_floor(band: usize, bands: usize, bass: f32) -> f32 {
    if band == 0 || band >= bands {
        return 0.0;
    }

    // Band 1 takes the full bass level, the last band takes none. With only two
    // bands there is no room for a ramp, so the neighbour rule wins.
    let span = (bands.saturating_sub(2)).max(1) as f32;
    let decay = ((band - 1) as f32 / span).clamp(0.0, 1.0);
    bass * (1.0 - decay)
}

/// How many bands fit across half the keyboard.
///
/// The display is mirrored about the vertical centre, so only half the columns
/// carry distinct bands. An odd width has a single shared centre column.
fn band_count(columns: usize) -> usize {
    columns.div_ceil(2)
}

/// The band a column belongs to: 0 at the centre, rising toward both edges.
///
/// Taking the centre as a fractional column handles both parities with no
/// special case — an even width has two columns at distance 0.5 which both floor
/// to band 0, an odd width has one column exactly at 0.
fn band_for_column(column: usize, columns: usize) -> usize {
    let centre = (columns - 1) as f32 / 2.0;
    let distance = (column as f32 - centre).abs();
    (distance.floor() as usize).min(band_count(columns) - 1)
}
