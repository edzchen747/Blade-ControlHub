#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SAMPLE_RATE: f64 = 48_000.0;
    const TICK: f32 = 1.0 / 60.0;
    /// An even width, narrower than `MATRIX_COLUMNS` so anything that assumed
    /// the protocol maximum instead of the probed width shows up here.
    const EVEN_COLUMNS: usize = 14;
    /// An odd width, which has a single shared centre column.
    const ODD_COLUMNS: usize = 15;

    fn matrix() -> EqualizerMatrix {
        EqualizerMatrix::new(EVEN_COLUMNS)
    }

    /// A matrix with its bass boost pinned, so height tests are independent of
    /// the trigger-and-expire envelope (which has its own tests).
    fn with_boost(bass: f32) -> EqualizerMatrix {
        let mut matrix = matrix();
        matrix.boost_bass = bass;
        matrix
    }

    /// Levels with `bass` in band 0 and `other` in every band above it.
    fn levels(bass: f32, other: f32, bands: usize) -> Vec<f32> {
        let mut levels = vec![other; bands];
        levels[0] = bass;
        levels
    }

    fn lit_columns_in(row: &[ThemeColor]) -> Vec<usize> {
        row.iter()
            .enumerate()
            .filter(|(_, c)| c.r.max(c.g).max(c.b) > 0)
            .map(|(i, _)| i)
            .collect()
    }

    fn is_lit(key: ThemeColor) -> bool {
        key.r.max(key.g).max(key.b) > 0
    }

    /// Feeds `ticks` analysis ticks of a mix of steady sines.
    fn feed(bank: &mut NoteBank, tones: &[(f64, f32)], ticks: usize) {
        let per_tick = (TEST_SAMPLE_RATE * TICK as f64) as usize;
        let mut phases = vec![0.0f64; tones.len()];

        for _ in 0..ticks {
            for _ in 0..per_tick {
                let mut sample = 0.0f32;
                for (index, (frequency, amplitude)) in tones.iter().enumerate() {
                    sample += phases[index].sin() as f32 * amplitude;
                    phases[index] += std::f64::consts::TAU * frequency / TEST_SAMPLE_RATE;
                }
                bank.samples_mut().push(sample);
            }
            bank.analyze(TICK);
        }
    }

    // -- Column geometry ----------------------------------------------------

    #[test]
    fn bands_cover_half_the_keyboard() {
        assert_eq!(band_count(EVEN_COLUMNS), 7);
        assert_eq!(band_count(ODD_COLUMNS), 8);
        // A full-width keyboard addresses columns 0..=18, so 19 columns.
        assert_eq!(band_count(MATRIX_COLUMNS), 10);
    }

    #[test]
    fn low_bands_sit_at_the_centre_and_high_bands_at_both_edges() {
        for columns in [EVEN_COLUMNS, ODD_COLUMNS] {
            let top = band_count(columns) - 1;

            assert_eq!(
                band_for_column(0, columns),
                top,
                "the left edge should carry the highest band at width {columns}"
            );
            assert_eq!(
                band_for_column(columns - 1, columns),
                top,
                "the right edge should carry the highest band at width {columns}"
            );
            assert_eq!(
                band_for_column(columns / 2, columns),
                0,
                "the centre should carry the bass band at width {columns}"
            );
        }
    }

    #[test]
    fn columns_mirror_about_the_vertical_axis() {
        for columns in [EVEN_COLUMNS, ODD_COLUMNS] {
            for column in 0..columns {
                assert_eq!(
                    band_for_column(column, columns),
                    band_for_column(columns - 1 - column, columns),
                    "column {column} is not mirrored at width {columns}"
                );
            }
        }
    }

    #[test]
    fn band_index_rises_monotonically_from_the_centre() {
        let columns = EVEN_COLUMNS;
        let mut previous = band_for_column(columns / 2, columns);
        for column in (columns / 2)..columns {
            let band = band_for_column(column, columns);
            assert!(band >= previous, "band dipped at column {column}");
            previous = band;
        }
    }

    // -- Frame shape --------------------------------------------------------

    #[test]
    fn a_frame_covers_every_addressable_key() {
        let matrix = matrix();
        let frame = matrix.render(&levels(0.0, 0.0, matrix.bands));

        assert_eq!(frame.len(), MATRIX_ROWS, "all seven protocol rows are sent");
        for row in &frame {
            assert_eq!(row.len(), EVEN_COLUMNS, "one colour per real LED");
        }
    }

    #[test]
    fn silence_leaves_the_whole_keyboard_dark() {
        let mut matrix = matrix();
        let quiet = levels(0.0, 0.0, matrix.bands);
        for _ in 0..400 {
            matrix.update(&quiet, &quiet, false, TICK);
        }
        let frame = matrix.render(&quiet);

        for (row, keys) in frame.iter().enumerate() {
            assert!(
                lit_columns_in(keys).is_empty(),
                "row {row} should be dark in silence"
            );
        }
    }

    // -- Vertical mirroring -------------------------------------------------

    #[test]
    fn bands_grow_outward_from_the_mirror_axis() {
        // One row per side: only the two rows against the axis should light.
        let mut matrix = matrix();
        let loud = levels(0.0, 1.0, matrix.bands);
        matrix.update(&loud, &loud, false, TICK);
        let frame = matrix.render(&loud);

        let column = 0; // an outer column, so it belongs to the top band
        let brightness =
            |row: usize| frame[row][column].r.max(frame[row][column].g).max(frame[row][column].b);

        // No bass, so no glow either: this is the bands on their own.
        assert_eq!(brightness(QUADRANT_ROWS - 1), 255, "row 2 should light first");
        assert_eq!(brightness(QUADRANT_ROWS), 255, "row 3 should light first");
        assert_eq!(brightness(1), 0, "row 1 is further out and should be dark");
        assert_eq!(brightness(4), 0, "row 4 is further out and should be dark");
        assert_eq!(brightness(0), 0);
        assert_eq!(brightness(5), 0);
    }

    #[test]
    fn the_two_halves_mirror_each_other_when_there_is_no_half_row() {
        let mut matrix = matrix();
        // No bass, so every band stands at exactly one row with no half-step
        // anywhere and no glow to blur the comparison.
        let loud = levels(0.0, 1.0, matrix.bands);
        matrix.update(&loud, &loud, false, TICK);
        let frame = matrix.render(&loud);

        // The centre block is excluded: its dither drops one key per column on
        // purpose, which breaks the vertical symmetry by design.
        for depth in 0..QUADRANT_ROWS {
            let top_row = QUADRANT_ROWS - 1 - depth;
            let bottom_row = QUADRANT_ROWS + depth;
            for (column, (top, bottom)) in frame[top_row]
                .iter()
                .zip(frame[bottom_row].iter())
                .enumerate()
            {
                if matrix.in_centre_block(top_row, column) {
                    continue;
                }
                assert_eq!(top, bottom, "depth {depth} column {column} is not mirrored");
            }
        }
    }

    // -- The bass multiplier ------------------------------------------------

    #[test]
    fn without_bass_nothing_exceeds_one_row() {
        let mut matrix = matrix();
        let no_bass = levels(0.0, 1.0, matrix.bands);
        matrix.update(&no_bass, &no_bass, false, TICK);

        for band in 0..matrix.bands {
            let (top, bottom) = matrix.band_rows(&no_bass, band);
            assert!(
                top <= 1 && bottom <= 1,
                "band {band} reached {top}/{bottom} rows with no bass"
            );
        }
    }

    #[test]
    fn full_bass_fills_the_quadrant_at_the_centre_but_not_at_the_edge() {
        let matrix = with_boost(1.0);
        let loud = levels(1.0, 1.0, matrix.bands);

        // The bass band itself is tripled, so it fills outright.
        assert_eq!(
            matrix.band_rows(&loud, 0),
            (QUADRANT_ROWS, QUADRANT_ROWS),
            "the centre should fill at full bass"
        );

        // The outermost band is only doubled, so it stops short.
        let (top, bottom) = matrix.band_rows(&loud, matrix.bands - 1);
        assert_eq!((top, bottom), (2, 2), "the edge should not fill");
    }

    #[test]
    fn the_bass_band_scales_itself_like_every_other_band() {
        let quiet_matrix = with_boost(0.0);
        // Band 0 is drawn, and because it is its own multiplier it collapses
        // with the rest when the bass drops.
        let quiet = levels(0.0, 0.0, quiet_matrix.bands);
        assert_eq!(quiet_matrix.band_rows(&quiet, 0), (0, 0));

        let loud_matrix = with_boost(1.0);
        let loud = levels(1.0, 0.0, loud_matrix.bands);
        assert_eq!(
            loud_matrix.band_rows(&loud, 0),
            (QUADRANT_ROWS, QUADRANT_ROWS)
        );
    }

    #[test]
    fn the_multiplier_lifts_every_band_not_just_the_bass() {
        let unboosted = with_boost(0.0);
        let boosted = with_boost(1.0);
        let band = unboosted.bands - 1;
        let bands = unboosted.bands;

        let without = unboosted.band_height(&levels(0.0, 0.5, bands), band);
        let with = boosted.band_height(&levels(1.0, 0.5, bands), band);

        // Even the outermost band is lifted, just by the smaller outer boost.
        assert!(
            (with - without * (1.0 + BASS_BOOST_OUTER)).abs() < 1e-5,
            "the edge band should get exactly the outer boost: {without} -> {with}"
        );
    }

    // -- The boost window ---------------------------------------------------

    /// Advances the matrix with an explicit hit flag.
    fn step(matrix: &mut EqualizerMatrix, bass: f32, hit: bool, seconds: f32) {
        let levels = levels(bass, 0.0, matrix.bands);
        matrix.update(&levels, &levels, hit, seconds);
    }

    #[test]
    fn a_hit_fires_a_boost() {
        let mut matrix = matrix();
        step(&mut matrix, 1.0, false, TICK);
        assert_eq!(matrix.boost_bass, 0.0, "no hit, no boost");

        step(&mut matrix, 1.0, true, 0.0);
        assert_eq!(matrix.boost_bass, 1.0, "a hit should boost at full");
    }

    #[test]
    fn the_boost_fades_across_its_window_and_expires() {
        let mut matrix = matrix();
        step(&mut matrix, 1.0, true, 0.0);

        step(&mut matrix, 1.0, false, BASS_BOOST_SECONDS / 2.0);
        assert!(
            (matrix.boost_bass - 0.5).abs() < 1e-5,
            "expected a half-faded boost, got {}",
            matrix.boost_bass
        );

        step(&mut matrix, 1.0, false, BASS_BOOST_SECONDS);
        assert_eq!(matrix.boost_bass, 0.0, "the window should run out");
    }

    #[test]
    fn a_held_note_without_hits_does_not_keep_boosting() {
        let mut matrix = matrix();
        step(&mut matrix, 1.0, true, 0.0);
        step(&mut matrix, 1.0, false, BASS_BOOST_SECONDS);

        for _ in 0..240 {
            step(&mut matrix, 1.0, false, TICK);
            assert_eq!(matrix.boost_bass, 0.0, "loud bass alone must not boost");
        }
    }

    #[test]
    fn each_hit_restarts_the_window() {
        let mut matrix = matrix();
        step(&mut matrix, 1.0, true, 0.0);
        step(&mut matrix, 1.0, false, BASS_BOOST_SECONDS * 0.8);
        let fading = matrix.boost_remaining;

        step(&mut matrix, 1.0, true, 0.0);
        assert!(
            matrix.boost_remaining > fading,
            "the next kick should restart the window: {fading} -> {}",
            matrix.boost_remaining
        );
    }

    #[test]
    fn the_bands_shrink_back_once_the_boost_expires() {
        // What it looks like on the keyboard: a hit lifts the bands, then lets
        // them settle even though the bass itself has not moved.
        let mut matrix = matrix();
        let loud = levels(1.0, 1.0, matrix.bands);

        matrix.update(&loud, &loud, true, 0.0);
        let boosted = matrix.band_height(&loud, 0);

        matrix.update(&loud, &loud, false, BASS_BOOST_SECONDS);
        let settled = matrix.band_height(&loud, 0);

        assert!((boosted - QUADRANT_ROWS as f32).abs() < 1e-5, "{boosted}");
        assert!(
            (settled - 1.0).abs() < 1e-5,
            "should fall back to its own level: {settled}"
        );
    }

    // -- Kick detection -----------------------------------------------------

    /// Feeds segments of `(hz, amplitude, ticks)` through the note bank and
    /// counts the hits detected during the final segment.
    /// As `hits_in_final_segment`, but each segment is a *mix* of tones.
    ///
    /// Needed to hold one low band busy while the other is struck, which is the
    /// case the split exists for and which a single tone cannot express.
    fn hits_in_final_mix(segments: &[(&[(f64, f32)], usize)]) -> usize {
        let mut bank = NoteBank::new(TEST_SAMPLE_RATE, band_count(EVEN_COLUMNS));
        let per_tick = (TEST_SAMPLE_RATE * TICK as f64) as usize;
        let mut phases = [0.0f64; 8];
        let mut hits = 0usize;

        for (index, (tones, ticks)) in segments.iter().copied().enumerate() {
            let measuring = index + 1 == segments.len();
            for _ in 0..ticks {
                for _ in 0..per_tick {
                    let mut sample = 0.0f32;
                    for (tone, (frequency, amplitude)) in tones.iter().copied().enumerate() {
                        sample += phases[tone].sin() as f32 * amplitude;
                        phases[tone] += std::f64::consts::TAU * frequency / TEST_SAMPLE_RATE;
                    }
                    bank.samples_mut().push(sample);
                }
                bank.analyze(TICK);
                if measuring && bank.bass_hit() {
                    hits += 1;
                }
            }
        }
        hits
    }

    fn hits_in_final_segment(segments: &[(f64, f32, usize)]) -> usize {
        let mut bank = NoteBank::new(TEST_SAMPLE_RATE, band_count(EVEN_COLUMNS));
        let per_tick = (TEST_SAMPLE_RATE * TICK as f64) as usize;
        let mut phase = 0.0f64;
        let mut hits = 0usize;

        for (index, (hz, amplitude, ticks)) in segments.iter().copied().enumerate() {
            let step = std::f64::consts::TAU * hz / TEST_SAMPLE_RATE;
            let measuring = index + 1 == segments.len();
            for _ in 0..ticks {
                for _ in 0..per_tick {
                    bank.samples_mut().push(phase.sin() as f32 * amplitude);
                    phase += step;
                }
                bank.analyze(TICK);
                if measuring && bank.bass_hit() {
                    hits += 1;
                }
            }
        }
        hits
    }

    #[test]
    fn the_bass_band_reaches_where_a_kick_lives() {
        // Fitted against a real capture: autocorrelating each band against the
        // beat put the strongest pulse at 144-260 Hz, so a bass band that stops
        // below that cannot see the kick it is meant to trigger on.
        let bank = NoteBank::new(TEST_SAMPLE_RATE, 9);

        const _: () = assert!(
            BASS_BAND_MAX_HZ >= 150.0,
            "the bass band must reach the kick body at ~150-250 Hz"
        );
        // And the band above it starts there rather than overlapping.
        assert!((bank.bins[BINS_PER_BAND].frequency - BASS_BAND_MAX_HZ).abs() < 0.01);
    }

    #[test]
    fn a_kick_registers_as_a_hit() {
        assert_eq!(
            hits_in_final_segment(&[(50.0, 0.0, 60), (50.0, 0.5, 6)]),
            1,
            "a kick out of silence should register exactly once"
        );
    }

    // -- Bass boost gate ----------------------------------------------------

    #[test]
    fn a_kick_under_the_minimum_level_does_not_fire_the_boost() {
        let mut matrix = EqualizerMatrix::new(MATRIX_COLUMNS);
        let quiet = BASS_BOOST_MIN_LEVEL * 0.5;

        matrix.update_boost(quiet, quiet, true, TICK);

        assert_eq!(
            matrix.boost_envelope, 0.0,
            "a kick with no weight behind it should register without boosting"
        );
    }

    #[test]
    fn a_kick_at_the_minimum_level_fires_the_boost() {
        let mut matrix = EqualizerMatrix::new(MATRIX_COLUMNS);
        let loud = BASS_BOOST_MIN_LEVEL;

        matrix.update_boost(loud, loud, true, TICK);

        assert!(
            matrix.boost_envelope > 0.0,
            "a kick that clears the bar should still boost"
        );
    }

    /// The gate reads the raw level, not the smoothed one.
    ///
    /// They diverge exactly when it matters: on the first tick of a sharp kick
    /// the envelope is still climbing, so judging on it would reject the very
    /// transients the boost exists for.
    #[test]
    fn the_boost_gate_judges_the_raw_level_not_the_smoothed_one() {
        let mut matrix = EqualizerMatrix::new(MATRIX_COLUMNS);
        let lagging = BASS_BOOST_MIN_LEVEL * 0.5;
        let actual = BASS_BOOST_MIN_LEVEL * 2.0;

        matrix.update_boost(lagging, actual, true, TICK);

        assert!(
            matrix.boost_envelope > 0.0,
            "a loud kick should boost even while the smoothed level trails it"
        );
    }

    // -- Output latency delay -----------------------------------------------

    /// Reads back the `count` most recent samples the ring holds, oldest first.
    fn tail(ring: &SampleRing, count: usize) -> Vec<f32> {
        (0..count).map(|i| ring.get_ending(0, count, i)).collect()
    }

    #[test]
    fn no_reported_latency_means_no_delay_at_all() {
        let mut ring = SampleRing::new();
        let mut delay = OutputDelay::new(0);

        for sample in [0.1, 0.2, 0.3] {
            delay.push(&mut ring, sample);
        }

        assert_eq!(ring.written(), 3, "samples should pass straight through");
        assert_eq!(tail(&ring, 3), vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn a_reported_latency_holds_the_first_samples_back() {
        let mut ring = SampleRing::new();
        let mut delay = OutputDelay::new(4);

        for sample in [0.1, 0.2, 0.3, 0.4] {
            delay.push(&mut ring, sample);
        }

        assert_eq!(
            ring.written(),
            0,
            "nothing should reach the analyser until the delay has filled"
        );
    }

    #[test]
    fn samples_emerge_in_order_once_the_delay_has_filled() {
        let mut ring = SampleRing::new();
        let mut delay = OutputDelay::new(4);

        for step in 1..=7 {
            delay.push(&mut ring, step as f32);
        }

        // Seven in, four held: the first three come out, oldest first.
        assert_eq!(ring.written(), 3);
        assert_eq!(tail(&ring, 3), vec![1.0, 2.0, 3.0]);
    }

    // -- Hue brightness floor -----------------------------------------------

    const RED: f32 = 0.0;
    const GREEN: f32 = 1.0 / 3.0;
    const CYAN: f32 = 0.5;
    const BLUE: f32 = 2.0 / 3.0;
    const VIOLET: f32 = 0.75;

    /// The colour a hue is actually drawn in, at full brightness.
    fn drawn(hue: f32) -> Rgb01 {
        hsv_to_rgb(hue, saturation_for_hue(hue), 1.0)
    }

    #[test]
    fn every_hue_in_the_cycle_clears_the_brightness_floor() {
        // Walked finely, because the cycle passes through all of them.
        for step in 0..360 {
            let hue = step as f32 / 360.0;
            let luminance = relative_luminance(drawn(hue));
            assert!(
                luminance >= MIN_HUE_LUMINANCE - 1e-3,
                "hue {hue:.3} is drawn at luminance {luminance:.3}, below the floor"
            );
        }
    }

    #[test]
    fn the_bright_hues_are_left_untouched() {
        for hue in [GREEN, CYAN, 1.0 / 6.0] {
            assert_eq!(
                saturation_for_hue(hue),
                1.0,
                "hue {hue:.3} is already bright and should keep full saturation"
            );
        }
    }

    #[test]
    fn the_dark_hues_are_lifted_rather_than_skipped() {
        for hue in [BLUE, VIOLET] {
            let saturation = saturation_for_hue(hue);
            assert!(
                saturation < 1.0,
                "hue {hue:.3} should be desaturated towards white"
            );
            assert!(
                saturation > 0.5,
                "hue {hue:.3} should stay recognisably itself, not wash out to white"
            );
        }
    }

    /// Lifting must not turn blue into something that is no longer blue.
    #[test]
    fn a_lifted_blue_is_still_blue() {
        let blue = drawn(BLUE);
        assert!(
            blue.b > blue.r && blue.b > blue.g,
            "bright blue should still lead on the blue channel, got {blue:?}"
        );
    }

    #[test]
    fn a_lifted_red_is_still_red() {
        let red = drawn(RED);
        assert!(
            red.r > red.g && red.r > red.b,
            "red should still lead on the red channel, got {red:?}"
        );
    }

    /// The flash still wins over the hue floor.
    #[test]
    fn a_flash_whites_out_even_a_hue_that_keeps_full_saturation() {
        let mut matrix = flashing_matrix();
        matrix.hue = GREEN;
        let loud = levels(1.0, 1.0, matrix.bands);
        let frame = matrix.render(&loud);

        let lit = frame
            .iter()
            .take(BAR_ROW)
            .flatten()
            .copied()
            .find(|key| is_lit(*key))
            .expect("the bands should be lit at full level");

        assert!(
            lit.r == lit.g && lit.g == lit.b,
            "the flash should override the hue entirely, got {:?}",
            (lit.r, lit.g, lit.b)
        );
    }

    // -- Boost flash --------------------------------------------------------

    /// A matrix mid-flash and nothing else.
    ///
    /// The flash is set directly rather than by firing a boost, because a real
    /// boost also raises `boost_bass`, which lights every band through the bass
    /// skirt. That would leave these tests unable to tell the flash apart from
    /// the skirt underneath it.
    fn flashing_matrix() -> EqualizerMatrix {
        let mut matrix = EqualizerMatrix::new(MATRIX_COLUMNS);
        matrix.flash_remaining = BAND_FLASH_SECONDS;
        matrix
    }

    #[test]
    fn a_boost_activation_starts_the_flash() {
        let mut matrix = EqualizerMatrix::new(MATRIX_COLUMNS);
        let loud = BASS_BOOST_MIN_LEVEL * 2.0;

        matrix.update_boost(loud, loud, true, 0.0);

        assert!(
            matrix.flash_envelope() > 0.99,
            "a boost should light the flash at full white"
        );
    }

    #[test]
    fn a_kick_too_quiet_to_boost_does_not_flash() {
        let mut matrix = EqualizerMatrix::new(MATRIX_COLUMNS);
        let quiet = BASS_BOOST_MIN_LEVEL * 0.5;

        matrix.update_boost(quiet, quiet, true, 0.0);

        assert_eq!(
            matrix.flash_envelope(),
            0.0,
            "the flash marks a boost, so a kick that does not boost must not flash"
        );
    }

    #[test]
    fn a_flash_draws_the_bands_white() {
        let matrix = flashing_matrix();
        let loud = levels(1.0, 1.0, matrix.bands);
        let frame = matrix.render(&loud);

        // Any band row, not row 0: without a boost the bass multiplier is 1, so
        // no band stands more than the single row nearest the centre.
        let lit = frame
            .iter()
            .take(BAR_ROW)
            .flatten()
            .copied()
            .find(|key| is_lit(*key))
            .expect("the bands should have something lit at full level");

        assert!(
            lit.r == lit.g && lit.g == lit.b,
            "a flashing band key should be neutral white, got {:?}",
            (lit.r, lit.g, lit.b)
        );
    }

    #[test]
    fn the_flash_leaves_the_bar_its_own_colour() {
        let mut matrix = flashing_matrix();
        // The bar's width comes from its own envelope, which `render` reads
        // rather than recomputes.
        matrix.bar_fill = 1.0;
        let loud = levels(1.0, 1.0, matrix.bands);
        let frame = matrix.render(&loud);

        let lit = frame[BAR_ROW]
            .iter()
            .copied()
            .find(|key| is_lit(*key))
            .expect("a full bar should be lit");

        assert!(
            lit.r != lit.g || lit.g != lit.b,
            "the bar keeps its hue through a flash: it is a meter, not a band"
        );
    }

    /// The flash must not light keys that were dark.
    ///
    /// Desaturating rather than overwriting is what buys this - `value` still
    /// carries the brightness, so a key at zero stays at zero.
    #[test]
    fn the_flash_does_not_light_a_dark_key() {
        let matrix = flashing_matrix();
        let quiet = levels(0.0, 0.0, matrix.bands);
        let frame = matrix.render(&quiet);

        assert!(
            frame.iter().flatten().all(|key| !is_lit(*key)),
            "a flash should wash out what is lit, not turn the board on"
        );
    }

    #[test]
    fn the_flash_fades_out_within_its_window() {
        let mut matrix = flashing_matrix();
        assert!(matrix.flash_envelope() > 0.99, "it starts fully white");

        matrix.update_boost(0.0, 0.0, false, BAND_FLASH_SECONDS * 0.5);
        let half = matrix.flash_envelope();
        assert!(
            half > 0.0 && half < 1.0,
            "it fades across the window rather than cutting out, got {half}"
        );

        matrix.update_boost(0.0, 0.0, false, BAND_FLASH_SECONDS);
        assert_eq!(matrix.flash_envelope(), 0.0, "and is gone by the end of it");
    }

    // -- Band selection -----------------------------------------------------

    /// Feeds a steady mix for long enough that the energy followers settle, and
    /// reports which band ended up with the floor.
    fn selected_band_after(tones: &[(f64, f32)], seconds: f32) -> usize {
        let mut bank = NoteBank::new(TEST_SAMPLE_RATE, band_count(EVEN_COLUMNS));
        feed(&mut bank, tones, (seconds / TICK).round() as usize);
        bank.onset_selected_band()
    }

    #[test]
    fn a_mix_with_no_sub_content_selects_the_punch_band() {
        // Nothing under 60 Hz at all, which is most guitar music.
        let selected = selected_band_after(&[(95.0, 0.5), (130.0, 0.4)], 8.0);
        assert_eq!(selected, 1, "with an empty sub band the punch range should hold the floor");
    }

    /// Starts from the punch band deliberately.
    ///
    /// Selection begins at band 0, so asserting "sub is selected" from a fresh
    /// bank would pass whether or not the rule works at all. Playing a sub-less
    /// mix first forces the floor away, so the assertion has something to prove.
    #[test]
    fn a_mix_with_strong_sub_content_takes_the_floor_back() {
        let mut bank = NoteBank::new(TEST_SAMPLE_RATE, band_count(EVEN_COLUMNS));
        let ticks = (8.0 / TICK).round() as usize;

        feed(&mut bank, &[(95.0, 0.5), (130.0, 0.4)], ticks);
        assert_eq!(
            bank.onset_selected_band(),
            1,
            "precondition: a sub-less mix should have moved the floor to the punch band"
        );

        feed(&mut bank, &[(35.0, 0.5), (95.0, 0.4)], ticks);
        assert_eq!(
            bank.onset_selected_band(),
            0,
            "a real sub-bass should take the floor back from the punch range"
        );
    }

    /// The Schmitt trigger, tested on the selection rule itself.
    ///
    /// Driving this with audio would prove nothing: the point is the behaviour
    /// between the two bars, and setting the energies directly is the only way
    /// to sit in that gap deliberately.
    #[test]
    fn selection_does_not_flap_between_the_two_bars() {
        let mut onset = OnsetBank::new(TEST_SAMPLE_RATE);

        // Sub clearly present: it takes the floor.
        onset.bands[0].energy = 1.0;
        onset.bands[1].energy = 1.0;
        onset.update_selection();
        assert_eq!(onset.selected, 0);

        // Sub sags to half the punch band - below ENGAGE but above RELEASE, so
        // an incumbent keeps its place where a challenger would not get in.
        onset.bands[0].energy = 0.5;
        onset.update_selection();
        assert_eq!(onset.selected, 0, "the incumbent should hold between the bars");

        // Below RELEASE it finally gives way.
        onset.bands[0].energy = 0.2;
        onset.update_selection();
        assert_eq!(onset.selected, 1, "below the release bar the floor should pass over");

        // And coming back, half is no longer enough to retake it.
        onset.bands[0].energy = 0.5;
        onset.update_selection();
        assert_eq!(onset.selected, 1, "a challenger needs the higher bar to take over");

        onset.bands[0].energy = 0.8;
        onset.update_selection();
        assert_eq!(onset.selected, 0, "clearing the engage bar should win the floor back");
    }

    #[test]
    fn selection_holds_when_everything_is_quiet() {
        let mut onset = OnsetBank::new(TEST_SAMPLE_RATE);
        onset.bands[0].energy = 1.0;
        onset.bands[1].energy = 1.0;
        onset.update_selection();
        assert_eq!(onset.selected, 0);

        // Silence between tracks must not re-decide the band on noise.
        onset.bands[0].energy = 0.0;
        onset.bands[1].energy = BAND_PRESENCE_FLOOR * 0.5;
        onset.update_selection();
        assert_eq!(onset.selected, 0, "a quiet gap should hold the selection, not re-judge it");
    }

    #[test]
    fn the_fallback_opens_only_after_the_selected_band_misses_a_beat() {
        let mut onset = OnsetBank::new(TEST_SAMPLE_RATE);
        onset.beat_interval_hops = 100.0;

        onset.hops_since_hit = 100;
        onset.update_fallback();
        assert!(!onset.in_fallback(), "one beat's silence is not a miss");

        onset.hops_since_hit = (100.0 * BAND_FALLBACK_GAPS) as usize + 1;
        onset.update_fallback();
        assert!(onset.in_fallback(), "past the gap the other bands should be let through");

        // A hit from the selected band closes it again.
        onset.hops_since_hit = 0;
        onset.update_fallback();
        assert!(!onset.in_fallback(), "the fallback should close once the band fires again");
    }

    /// The punch band has to fire on its own merits.
    ///
    /// A loud steady sub tone runs throughout, so under a single wide low band
    /// its magnitude would dominate the shared median and a 100 Hz strike would
    /// have to clear a bar set by something it has nothing to do with.
    #[test]
    fn a_punch_kick_registers_under_a_steady_sub_tone() {
        let quiet: &[(f64, f32)] = &[(35.0, 0.45)];
        let struck: &[(f64, f32)] = &[(35.0, 0.45), (100.0, 0.5)];
        assert_eq!(
            hits_in_final_mix(&[(quiet, 60), (struck, 6)]),
            1,
            "a punch-range kick should register even with the sub band held busy"
        );
    }

    /// And the sub band likewise, which is the case that prompted the split:
    /// a drop under a mix whose punch range never moves.
    #[test]
    fn a_sub_drop_registers_under_a_steady_punch_tone() {
        let quiet: &[(f64, f32)] = &[(110.0, 0.45)];
        let struck: &[(f64, f32)] = &[(110.0, 0.45), (40.0, 0.5)];
        assert_eq!(
            hits_in_final_mix(&[(quiet, 60), (struck, 6)]),
            1,
            "a sub drop should register even with the punch band held busy"
        );
    }

    #[test]
    fn a_quiet_kick_registers_just_the_same() {
        // Detection is on the rise and on bass dominance, both of which are
        // relative, so a quiet kick counts as readily as a loud one.
        assert_eq!(
            hits_in_final_segment(&[(50.0, 0.0, 60), (50.0, 0.12, 6)]),
            1,
            "a quiet kick should still register"
        );
    }

    #[test]
    fn a_held_bass_note_stops_registering() {
        // The same tone as a kick, just sustained. Only the attack differs.
        assert_eq!(
            hits_in_final_segment(&[(50.0, 0.5, 90), (50.0, 0.5, 120)]),
            0,
            "a sustained bass note is not a drum"
        );
    }



    #[test]
    fn a_gradual_swell_produces_no_hits() {
        // The trough test in isolation: energy that arrives gradually is never
        // an onset, however loud it ends up. This is the mechanism that keeps a
        // sustained or swelling voice from firing the boost.
        let mut segments: Vec<(f64, f32, usize)> = vec![(60.0, 0.0, 30)];
        for step in 1..=20 {
            segments.push((60.0, 0.03 * step as f32, 6));
        }
        assert_eq!(
            hits_in_final_segment(&segments),
            0,
            "a gradual swell is not a kick"
        );
    }


    #[test]
    fn a_repeated_kick_pattern_registers_every_beat() {
        // Four kicks, each measured on its own, to show detection is consistent
        // rather than latching on the first and missing the rest.
        for beat in 0..4 {
            let mut pattern = vec![(50.0f64, 0.0f32, 30usize)];
            for _ in 0..beat {
                pattern.push((50.0, 0.5, 5));
                pattern.push((50.0, 0.0, 25));
            }
            pattern.push((50.0, 0.5, 5));

            assert_eq!(
                hits_in_final_segment(&pattern),
                1,
                "beat {beat} should register"
            );
        }
    }

    #[test]
    fn one_kick_does_not_register_twice() {
        // A kick spans many analysis ticks as its energy crosses the window;
        // the refractory period keeps that to a single hit.
        assert_eq!(
            hits_in_final_segment(&[(50.0, 0.0, 60), (50.0, 0.5, 20)]),
            1,
            "a single kick should count once"
        );
    }

    // -- The bass skirt -----------------------------------------------------

    #[test]
    fn the_bass_lifts_its_neighbour_to_its_own_level() {
        let bands = band_count(EVEN_COLUMNS);

        assert!((band_floor(1, bands, 1.0) - 1.0).abs() < 1e-6);
        assert!((band_floor(1, bands, 0.4) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn the_lift_falls_away_to_nothing_at_the_outermost_band() {
        let bands = band_count(EVEN_COLUMNS);

        assert_eq!(band_floor(bands - 1, bands, 1.0), 0.0);
        // Never lifts the bass band itself.
        assert_eq!(band_floor(0, bands, 1.0), 0.0);
    }

    #[test]
    fn the_lift_decreases_linearly_outward() {
        let bands = band_count(EVEN_COLUMNS);
        let floors: Vec<f32> = (1..bands).map(|b| band_floor(b, bands, 1.0)).collect();

        for pair in floors.windows(2) {
            assert!(pair[1] < pair[0], "the skirt should fall outward: {floors:?}");
        }

        // Linear: every step down is the same size.
        let first_step = floors[0] - floors[1];
        for pair in floors.windows(2) {
            assert!(
                (pair[0] - pair[1] - first_step).abs() < 1e-5,
                "steps are uneven: {floors:?}"
            );
        }
    }

    #[test]
    fn no_bass_means_no_floor_at_all() {
        let bands = band_count(EVEN_COLUMNS);
        for band in 0..bands {
            assert_eq!(band_floor(band, bands, 0.0), 0.0);
        }

        // The "one row maximum without bass" rule therefore still holds.
        let mut matrix = matrix();
        let no_bass = levels(0.0, 0.0, matrix.bands);
        matrix.update(&no_bass, &no_bass, false, TICK);
        for band in 0..matrix.bands {
            assert_eq!(matrix.band_rows(&no_bass, band), (0, 0));
        }
    }

    #[test]
    fn a_silent_neighbour_still_fills_up_on_a_full_bass_hit() {
        // Band 1 carries no signal of its own; the bass alone should raise it.
        let mut matrix = matrix();
        let bass_only = levels(1.0, 0.0, matrix.bands);
        matrix.update(&bass_only, &bass_only, true, 0.0);

        // Lifted to the top of its range, give or take the half-step the taper
        // leaves it on: the adjacent band no longer quite fills, because the
        // boost has already begun tapering off by band 1.
        let (top, bottom) = matrix.band_rows(&bass_only, 1);
        assert!(
            top + bottom >= 2 * QUADRANT_ROWS - 1,
            "a full bass hit should nearly fill the adjacent band, got {top}/{bottom}"
        );
    }

    #[test]
    fn the_skirt_tapers_across_the_keyboard() {
        let mut matrix = matrix();
        let bass_only = levels(1.0, 0.0, matrix.bands);
        matrix.update(&bass_only, &bass_only, true, 0.0);

        let heights: Vec<f32> = (1..matrix.bands)
            .map(|band| matrix.band_height(&bass_only, band))
            .collect();

        for pair in heights.windows(2) {
            assert!(
                pair[1] <= pair[0],
                "the skirt should never grow outward: {heights:?}"
            );
        }
        assert_eq!(*heights.last().expect("at least one band"), 0.0);
    }

    #[test]
    fn a_bands_own_signal_still_wins_when_it_is_louder() {
        // The floor is a minimum, not an override.
        let matrix = matrix();
        let band = matrix.bands - 2;
        let mut quiet_bass = levels(0.2, 0.0, matrix.bands);
        quiet_bass[band] = 1.0;

        assert!(
            matrix.band_height(&quiet_bass, band)
                > matrix.band_height(&levels(0.2, 0.0, matrix.bands), band)
        );
    }

    #[test]
    fn the_bass_boost_tapers_from_the_centre_to_the_edge() {
        let bands = band_count(EVEN_COLUMNS);

        assert!((bass_boost(0, bands) - BASS_BOOST_CENTRE).abs() < 1e-6);
        assert!((bass_boost(bands - 1, bands) - BASS_BOOST_OUTER).abs() < 1e-6);

        // Falling the whole way, in even steps.
        let boosts: Vec<f32> = (0..bands).map(|b| bass_boost(b, bands)).collect();
        let first_step = boosts[0] - boosts[1];
        for pair in boosts.windows(2) {
            assert!(pair[1] < pair[0], "the boost should taper: {boosts:?}");
            assert!(
                (pair[0] - pair[1] - first_step).abs() < 1e-5,
                "the taper should be linear: {boosts:?}"
            );
        }
    }

    #[test]
    fn the_edge_band_is_doubled_where_the_centre_is_tripled() {
        let matrix = with_boost(1.0);
        let loud = levels(1.0, 1.0, matrix.bands);

        // Both bands carry the same level, so only the boost separates them.
        let centre = matrix.band_height(&loud, 0);
        let edge = matrix.band_height(&loud, matrix.bands - 1);

        assert!((centre - QUADRANT_ROWS as f32).abs() < 1e-5, "centre: {centre}");
        assert!((edge - (1.0 + BASS_BOOST_OUTER)).abs() < 1e-5, "edge: {edge}");
    }

    // -- The half-row -------------------------------------------------------

    #[test]
    fn a_half_step_lights_one_extra_row_on_exactly_one_side() {
        let mut matrix = matrix();
        // No boost at all: a band at level 0.5 stands half a row tall, which is
        // the half-step in its simplest form.
        let half = levels(0.0, 0.5, matrix.bands);
        matrix.update(&half, &half, false, TICK);

        let band = matrix.bands - 1;
        assert!((matrix.band_height(&half, band) - 0.5).abs() < 1e-6);

        let (top, bottom) = matrix.band_rows(&half, band);
        assert_eq!(top + bottom, 1, "half a row each side is one row in total");
        assert!(
            (top == 1 && bottom == 0) || (top == 0 && bottom == 1),
            "the extra row must land on exactly one side, got {top}/{bottom}"
        );
    }

    #[test]
    fn the_half_row_side_is_held_while_the_half_lasts() {
        let mut matrix = matrix();
        let half = levels(0.0, 0.5, matrix.bands);
        matrix.update(&half, &half, false, TICK);

        let band = matrix.bands - 1;
        let chosen = matrix.half_sides[band];
        assert!(chosen.is_some(), "a half-row should have picked a side");

        // Re-rolling per frame would strobe the key at the frame rate.
        for _ in 0..120 {
            matrix.update(&half, &half, false, TICK);
            assert_eq!(matrix.half_sides[band], chosen, "the side flipped mid-half");
        }
    }

    #[test]
    fn the_half_row_side_is_released_when_the_half_goes_away() {
        let mut matrix = matrix();
        let band = matrix.bands - 1;

        let half = levels(0.0, 0.5, matrix.bands);
        matrix.update(&half, &half, false, TICK);
        assert!(matrix.half_sides[band].is_some());

        let whole = levels(0.0, 1.0, matrix.bands);
        matrix.update(&whole, &whole, false, TICK);
        assert_eq!(
            matrix.half_sides[band], None,
            "a whole-row band should not hold a side"
        );

        matrix.update(&half, &half, false, TICK);
        assert!(
            matrix.half_sides[band].is_some(),
            "the side should be drawn again when the half returns"
        );
    }

    #[test]
    fn a_half_row_makes_the_halves_differ_by_one_row() {
        let mut matrix = matrix();
        let half = levels(0.0, 0.5, matrix.bands);
        matrix.update(&half, &half, false, TICK);
        let frame = matrix.render(&half);

        let driven = |row: usize| frame[row][0].r.max(frame[row][0].g).max(frame[row][0].b) == 255;
        let top_lit = (0..QUADRANT_ROWS).filter(|row| driven(*row)).count();
        let bottom_lit = (QUADRANT_ROWS..VISUALIZER_ROWS).filter(|row| driven(*row)).count();

        assert_eq!(top_lit.abs_diff(bottom_lit), 1);
    }

    // -- Colour -------------------------------------------------------------

    #[test]
    fn the_whole_keyboard_is_one_colour() {
        let mut matrix = matrix();
        let loud = levels(0.6, 0.8, matrix.bands);
        for _ in 0..30 {
            matrix.update(&loud, &loud, false, TICK);
        }

        let frame = matrix.render(&loud);
        let lit: Vec<ThemeColor> = frame
            .iter()
            .flatten()
            .copied()
            .filter(|key| is_lit(*key))
            .collect();
        assert!(!lit.is_empty(), "nothing lit up at all");

        // Compared against the *brightest* key, so every scale factor is <= 1.
        // Scaling a dim key up instead multiplies its byte-rounding error up
        // with it: `MIN_HUE_LUMINANCE` means the small channels are no longer
        // exact zeros, so a key like (31, 4, 2) carries ~12% error per channel
        // that an 8x scale turns into a false failure.
        let brightest = lit
            .iter()
            .copied()
            .max_by_key(|key| key.r.max(key.g).max(key.b))
            .expect("checked non-empty");
        let peak = brightest.r.max(brightest.g).max(brightest.b) as f32;

        for key in lit {
            // Brightness varies - bloom and the centre block are both partial -
            // but the hue must not. Saturation is deliberately not checked: the
            // luminance floor mixes white into the darker hues on purpose.
            let scale = key.r.max(key.g).max(key.b) as f32 / peak;
            for (channel, reference) in [
                (key.r, brightest.r),
                (key.g, brightest.g),
                (key.b, brightest.b),
            ] {
                assert!(
                    (channel as f32 - reference as f32 * scale).abs() <= 2.0,
                    "the board should be a single hue: {key:?} vs {brightest:?}"
                );
            }
        }
    }

    #[test]
    fn the_colour_cycles_the_rainbow_and_wraps() {
        let mut matrix = matrix();
        let quiet = levels(0.0, 0.0, matrix.bands);

        matrix.update(&quiet, &quiet, false, 1.0);
        assert!((matrix.hue - 1.0 / RAINBOW_PERIOD_SECONDS).abs() < 1e-6);

        matrix.update(&quiet, &quiet, false, RAINBOW_PERIOD_SECONDS);
        assert!(
            matrix.hue >= 0.0 && matrix.hue < 1.0,
            "hue left the circle: {}",
            matrix.hue
        );
    }

    // -- The bass bar -------------------------------------------------------

    /// Runs the bar envelope to rest against a steady input.
    fn settled_bar(levels: &[f32]) -> EqualizerMatrix {
        let mut matrix = matrix();
        for _ in 0..400 {
            matrix.update(levels, levels, false, TICK);
        }
        matrix
    }

    #[test]
    fn the_bar_drops_to_nothing_with_no_audio() {
        let matrix = matrix();
        let quiet = levels(0.0, 0.0, matrix.bands);

        assert_eq!(bar_target(&quiet, 0.0, true), 0.0);
        assert_eq!(settled_bar(&quiet).bar_columns(), 0);
    }

    #[test]
    fn a_short_gap_holds_the_bar_at_its_floor_instead_of_going_dark() {
        let matrix = matrix();
        let quiet = levels(0.0, 0.0, matrix.bands);

        assert_eq!(
            bar_target(&quiet, 0.0, false),
            BAR_MIN_FILL,
            "before the delay is up, quiet should read as a gap and hold the floor"
        );
    }

    /// The delay has to be an unbroken run, not a total.
    #[test]
    fn silence_is_only_deemed_after_the_delay_and_any_audio_resets_it() {
        let mut matrix = matrix();
        let quiet = levels(0.0, 0.0, matrix.bands);
        let loud = levels(0.8, 0.5, matrix.bands);

        matrix.silence_elapsed = 0.0;
        matrix.update_silence(&quiet, BAR_SILENCE_DELAY_SECONDS * 0.5);
        assert!(!matrix.silence_settled(), "half the delay is not yet silence");

        // A single frame of audio puts the clock back to the start.
        matrix.update_silence(&loud, BAR_SILENCE_DELAY_SECONDS * 0.5);
        assert!(!matrix.silence_settled(), "audio should reset the silence clock");

        matrix.update_silence(&quiet, BAR_SILENCE_DELAY_SECONDS * 0.75);
        assert!(
            !matrix.silence_settled(),
            "the delay measures an unbroken run, not a running total"
        );

        matrix.update_silence(&quiet, BAR_SILENCE_DELAY_SECONDS * 0.5);
        assert!(matrix.silence_settled(), "past the delay it is silence");
    }

    /// A fresh matrix must start settled.
    ///
    /// Otherwise the effect would paint a lit stub for half a second every time
    /// it starts against a system that is not playing anything.
    #[test]
    fn a_new_matrix_starts_already_deeming_silence() {
        assert!(
            matrix().silence_settled(),
            "starting unsettled would light a stub on a silent system"
        );
    }

    #[test]
    fn the_bar_still_holds_its_floor_while_anything_is_playing() {
        let matrix = matrix();
        // Barely any signal at all, but not silence.
        let mut whisper = levels(0.0, 0.0, matrix.bands);
        whisper[2] = 0.001;

        let target = bar_target(&whisper, 0.0, true);
        assert!(
            (BAR_MIN_FILL..BAR_MIN_FILL + 0.01).contains(&target),
            "a whisper should sit at the floor, not below or well above: {target}"
        );
        assert!(settled_bar(&whisper).bar_columns() >= 1);
    }

    #[test]
    fn bass_is_an_ordinary_band_until_a_kick_promotes_it() {
        let matrix = matrix();
        let loud = levels(1.0, 0.0, matrix.bands);

        // Off the boost it competes with the rest at the ordinary share.
        let resting = bar_target(&loud, 0.0, true);
        let expected = BAR_MIN_FILL + BAR_OTHER_SHARE * BAR_BASS_SPAN;
        assert!(
            (resting - expected).abs() < 1e-5,
            "full bass with no kick should sit at the ordinary share: {resting}"
        );

        // A kick hands it the whole span.
        let promoted = bar_target(&loud, 1.0, true);
        assert!(
            (promoted - (BAR_MIN_FILL + BAR_BASS_SPAN)).abs() < 1e-5,
            "a kick should take the bar to the top of the span: {promoted}"
        );
        assert!(promoted > resting * 2.0, "{resting} -> {promoted}");
    }

    #[test]
    fn off_the_boost_the_bar_follows_whichever_band_is_loudest() {
        let matrix = matrix();
        let bands = matrix.bands;

        // Same loudest value, whether it is the bass or something above it.
        let from_bass = bar_target(&levels(0.8, 0.2, bands), 0.0, true);
        let from_mids = bar_target(&levels(0.2, 0.8, bands), 0.0, true);
        assert!(
            (from_bass - from_mids).abs() < 1e-5,
            "the bass should get no special treatment off the boost: {from_bass} vs {from_mids}"
        );

        // And a quieter board gives a lower bar.
        assert!(bar_target(&levels(0.3, 0.3, bands), 0.0, true) < from_bass);
    }

    #[test]
    fn the_bar_is_anchored_at_the_left_edge() {
        let matrix = matrix();
        let loud = levels(0.5, 0.0, matrix.bands);
        let matrix = settled_bar(&loud);
        let frame = matrix.render(&loud);

        let lit = lit_columns_in(&frame[BAR_ROW]);
        assert!(!lit.is_empty());
        // A left-anchored bar is exactly the prefix 0..n.
        assert_eq!(lit, (0..lit.len()).collect::<Vec<_>>());
    }

    #[test]
    fn the_bar_bass_is_boosted_while_the_window_is_open() {
        let bands = band_count(EVEN_COLUMNS);
        let kick = levels(0.3, 0.0, bands);

        let plain = bar_target(&kick, 0.0, true);
        let punched = bar_target(&kick, 1.0, true);

        // +200% on the bass term, so a third-strength kick reads as near full.
        let expected = BAR_MIN_FILL + (0.3 * (1.0 + BAR_BASS_BOOST)) * BAR_BASS_SPAN;
        assert!((punched - expected).abs() < 1e-5, "{punched} vs {expected}");
        assert!(punched > plain, "{plain} -> {punched}");
    }

    #[test]
    fn the_bar_boost_fades_with_the_envelope() {
        let bands = band_count(EVEN_COLUMNS);
        let kick = levels(0.3, 0.0, bands);

        let full = bar_target(&kick, 1.0, true);
        let half = bar_target(&kick, 0.5, true);
        let gone = bar_target(&kick, 0.0, true);

        assert!(full > half && half > gone, "{gone} / {half} / {full}");
    }

    #[test]
    fn the_bar_boost_cannot_overfill_the_row() {
        let bands = band_count(EVEN_COLUMNS);
        // 0.5 boosted by 200% would be 1.5 without the cap.
        let target = bar_target(&levels(0.5, 0.0, bands), 1.0, true);

        assert!(
            (target - (BAR_MIN_FILL + BAR_BASS_SPAN)).abs() < 1e-5,
            "a boosted kick should fill to the bass span and no further: {target}"
        );
        assert!(target <= 1.0);
    }

    #[test]
    fn the_bar_punches_on_a_kick_then_settles_while_the_note_is_held() {
        let mut matrix = matrix();
        let bass = 0.6;
        let held = levels(bass, 0.0, matrix.bands);

        // Fire the boost on a hit, then let the bar catch up while the window
        // is still well open.
        // Sampled early: the bar chases a target that is already fading, so it
        // never quite reaches the instantaneous peak.
        matrix.update(&held, &held, true, 0.0);
        for _ in 0..5 {
            matrix.update(&held, &held, false, TICK);
        }
        let punched = matrix.bar_fill;

        // The bass never moves, but the boost window runs out.
        for _ in 0..60 {
            matrix.update(&held, &held, false, BASS_BOOST_SECONDS / 10.0);
        }
        let settled = matrix.bar_fill;

        let resting = BAR_MIN_FILL + bass * BAR_OTHER_SHARE * BAR_BASS_SPAN;
        assert!(
            punched > BAR_MIN_FILL + BAR_BASS_SPAN * 0.75,
            "a kick should punch most of the way up the span: {punched}"
        );
        assert!(
            (settled - resting).abs() < 0.02,
            "and settle back to the ordinary share: {settled} vs {resting}"
        );
        assert!(settled < punched * 0.5, "{punched} -> {settled}");
    }

    #[test]
    fn the_bar_grows_with_bass() {
        let matrix = matrix();
        let quiet = settled_bar(&levels(0.2, 0.0, matrix.bands)).bar_columns();
        let loud = settled_bar(&levels(0.8, 0.0, matrix.bands)).bar_columns();

        assert!(loud > quiet, "{quiet} -> {loud}");
    }

    #[test]
    fn the_bar_falls_faster_than_the_quadrant_bands() {
        let matrix = matrix();
        let loud = levels(1.0, 1.0, matrix.bands);
        let quiet = levels(0.0, 0.0, matrix.bands);

        let mut matrix = settled_bar(&loud);
        let started_at = matrix.bar_fill;

        // The input stops, but the smoothed band levels are still high — which
        // is exactly the case where a release applied on top of them would have
        // done nothing, because the bar would just track their slow decay.
        const TICKS: usize = 10;
        for _ in 0..TICKS {
            matrix.update(&loud, &quiet, false, TICK);
        }

        // What the band smoothing alone would have managed over the same span.
        let mut as_slow_as_a_band = started_at;
        for _ in 0..TICKS {
            as_slow_as_a_band += (BAR_MIN_FILL - as_slow_as_a_band) * LEVEL_RELEASE;
        }

        assert!(
            matrix.bar_fill < as_slow_as_a_band,
            "the bar should outrun the band release: {} vs {as_slow_as_a_band}",
            matrix.bar_fill
        );
    }

    #[test]
    fn the_bar_still_rises_promptly() {
        let matrix = matrix();
        let loud = levels(1.0, 0.0, matrix.bands);
        let mut matrix = matrix;

        for _ in 0..6 {
            matrix.update(&loud, &loud, false, TICK);
        }

        assert!(
            matrix.bar_fill > bar_target(&loud, 0.0, true) * 0.9,
            "a faster fall must not cost it the attack: {}",
            matrix.bar_fill
        );
    }

    // -- Bloom --------------------------------------------------------------

    /// The band a column belongs to on the test keyboard.
    fn band_at(column: usize) -> usize {
        band_for_column(column, EVEN_COLUMNS)
    }

    /// A grid with a single lit key at `(row, column)`.
    fn one_lit_key(row: usize, column: usize) -> Vec<Vec<f32>> {
        let mut values = vec![vec![0.0f32; EVEN_COLUMNS]; MATRIX_ROWS];
        values[row][column] = 1.0;
        values
    }

    /// A centre column, which sits in the bass band and so glows at full strength.
    const CENTRE_COLUMN: usize = EVEN_COLUMNS / 2;

    #[test]
    fn the_glow_scales_from_nothing_to_full_with_the_bass() {
        let bands = band_count(EVEN_COLUMNS);

        assert_eq!(bloom_strength(0, bands, 0.0), 0.0);
        assert!((bloom_strength(0, bands, 1.0) - BLOOM_STRENGTH).abs() < 1e-6);
        assert!((bloom_strength(0, bands, 0.5) - BLOOM_STRENGTH / 2.0).abs() < 1e-6);
    }

    #[test]
    fn the_glow_tapers_from_the_bass_band_out_to_the_edges() {
        let bands = band_count(EVEN_COLUMNS);

        // Full strength where the bass is drawn...
        assert!((bloom_strength(0, bands, 1.0) - BLOOM_STRENGTH).abs() < 1e-6);
        // ...down to the outer ceiling at the highest band.
        assert!((bloom_strength(bands - 1, bands, 1.0) - BLOOM_STRENGTH_OUTER).abs() < 1e-6);

        // Falling the whole way, in even steps.
        let ceilings: Vec<f32> = (0..bands).map(|b| bloom_strength(b, bands, 1.0)).collect();
        let first_step = ceilings[0] - ceilings[1];
        for pair in ceilings.windows(2) {
            assert!(pair[1] < pair[0], "the taper should fall: {ceilings:?}");
            assert!(
                (pair[0] - pair[1] - first_step).abs() < 1e-5,
                "the taper should be linear: {ceilings:?}"
            );
        }
    }

    #[test]
    fn the_taper_scales_with_the_bass_too() {
        let bands = band_count(EVEN_COLUMNS);

        // Half bass halves every band's ceiling, edges included.
        for band in 0..bands {
            let half = bloom_strength(band, bands, 0.5);
            let full = bloom_strength(band, bands, 1.0);
            assert!((half - full / 2.0).abs() < 1e-6, "band {band}");
        }
    }

    #[test]
    fn no_bass_means_no_glow_at_all() {
        let mut matrix = matrix();
        let no_bass = levels(0.0, 1.0, matrix.bands);
        matrix.update(&no_bass, &no_bass, false, TICK);
        let frame = matrix.render(&no_bass);

        // Only the two rows against the axis are driven, and with no bass
        // nothing spills off them.
        for row in [0usize, 1, 4, 5] {
            assert!(
                lit_columns_in(&frame[row]).is_empty(),
                "row {row} should be dark without bass"
            );
        }
    }

    #[test]
    fn full_bass_glows_onto_neighbouring_keys() {
        let matrix = matrix();
        let mut values = one_lit_key(2, CENTRE_COLUMN);
        matrix.apply_bloom(&mut values, 1.0);

        assert_eq!(band_at(CENTRE_COLUMN), 0, "the centre column is the bass band");
        assert_eq!(values[1][CENTRE_COLUMN], BLOOM_STRENGTH);
    }

    #[test]
    fn keys_further_from_the_bass_glow_less() {
        let matrix = matrix();
        // Light the whole row, so every key has an equally bright neighbour and
        // only the taper can distinguish them.
        let mut values = vec![vec![0.0f32; EVEN_COLUMNS]; MATRIX_ROWS];
        values[2] = vec![1.0; EVEN_COLUMNS];
        matrix.apply_bloom(&mut values, 1.0);

        let centre = values[1][CENTRE_COLUMN];
        let edge = values[1][0];

        assert!(band_at(0) > band_at(CENTRE_COLUMN));
        assert!(edge < centre, "the edge should glow less: {edge} vs {centre}");
        assert!((edge - BLOOM_STRENGTH_OUTER).abs() < 1e-6);
    }

    #[test]
    fn the_glow_does_not_chain_outward() {
        // Spill is measured against the pre-bloom frame, so a glowing key does
        // not go on to light its own neighbours.
        let matrix = matrix();
        let mut values = one_lit_key(2, CENTRE_COLUMN);
        matrix.apply_bloom(&mut values, 1.0);

        assert!(values[2][CENTRE_COLUMN + 1] > 0.0, "the neighbour should glow");
        assert_eq!(
            values[2][CENTRE_COLUMN + 2],
            0.0,
            "but the key beyond it should not"
        );
    }

    #[test]
    fn the_glow_takes_the_brightest_neighbour_not_the_sum() {
        // A key with a lit neighbour either side must not add them together.
        let matrix = matrix();
        let mut values = one_lit_key(2, CENTRE_COLUMN - 1);
        values[2][CENTRE_COLUMN + 1] = 1.0;
        matrix.apply_bloom(&mut values, 1.0);

        assert_eq!(
            values[2][CENTRE_COLUMN], BLOOM_STRENGTH,
            "glow should not accumulate"
        );
    }

    #[test]
    fn the_glow_never_dims_a_key_that_is_already_brighter() {
        let matrix = matrix();
        let mut values = one_lit_key(2, CENTRE_COLUMN);
        values[2][CENTRE_COLUMN + 1] = 1.0;
        matrix.apply_bloom(&mut values, 1.0);

        assert_eq!(values[2][CENTRE_COLUMN], 1.0);
        assert_eq!(values[2][CENTRE_COLUMN + 1], 1.0);
    }

    #[test]
    fn the_bar_row_is_left_out_of_the_bloom_pass() {
        let matrix = matrix();
        let mut values = vec![vec![0.0f32; EVEN_COLUMNS]; MATRIX_ROWS];
        values[BAR_ROW][CENTRE_COLUMN] = 1.0;
        values[VISUALIZER_ROWS - 1][CENTRE_COLUMN + 1] = 1.0;

        matrix.apply_bloom(&mut values, 1.0);

        assert_eq!(
            values[VISUALIZER_ROWS - 1][CENTRE_COLUMN],
            BLOOM_STRENGTH,
            "this key glows from its own row, not from the bar below it"
        );
        assert_eq!(
            values[BAR_ROW][CENTRE_COLUMN + 1],
            0.0,
            "the bar should not receive glow from the bands"
        );
        assert_eq!(
            values[BAR_ROW][CENTRE_COLUMN], 1.0,
            "the bar's own key is untouched"
        );
    }

    #[test]
    fn the_bar_stays_exactly_as_wide_as_its_fill() {
        let matrix = matrix();
        let loud = levels(1.0, 0.0, matrix.bands);
        let matrix = settled_bar(&loud);
        let frame = matrix.render(&loud);

        assert_eq!(lit_columns_in(&frame[BAR_ROW]).len(), matrix.bar_columns());
    }

    // -- Block dither -------------------------------------------------------

    /// Drives the matrix until the block sits at `brightness`, then returns the
    /// keys of the block that are lit.
    fn block_keys_at(matrix: &mut EqualizerMatrix, brightness: f32) -> Vec<(usize, usize)> {
        // Work back from the block transform to the bar fill that produces it.
        matrix.bar_fill = (brightness + CENTRE_BLOCK_OFFSET) / CENTRE_BLOCK_SCALE;

        let levels = levels(0.0, 0.0, matrix.bands);
        let frame = matrix.render(&levels);
        let columns = matrix.centre_block_columns();
        let half = CENTRE_BLOCK_HEIGHT / 2;

        let mut lit = Vec::new();
        for (row, keys) in frame
            .iter()
            .enumerate()
            .take(QUADRANT_ROWS + half)
            .skip(QUADRANT_ROWS - half)
        {
            for column in columns.clone() {
                if is_lit(keys[column]) {
                    lit.push((row, column));
                }
            }
        }
        lit
    }

    #[test]
    fn a_full_block_drops_nothing() {
        let mut matrix = matrix();
        matrix.update_dither(BLOCK_DITHER_SECONDS);

        assert_eq!(
            block_keys_at(&mut matrix, 1.0).len(),
            CENTRE_BLOCK_WIDTH * CENTRE_BLOCK_HEIGHT,
            "at full brightness the probability of dropping is zero"
        );
    }

    #[test]
    fn a_dim_block_still_dissolves() {
        // It keeps breaking up all the way down rather than switching to a
        // plain fade at half brightness.
        let mut matrix = matrix();
        let total = CENTRE_BLOCK_WIDTH * CENTRE_BLOCK_HEIGHT;
        let mut dropped = 0usize;

        for _ in 0..60 {
            matrix.update_dither(BLOCK_DITHER_SECONDS);
            dropped += total - block_keys_at(&mut matrix, 0.15).len();
        }

        assert!(dropped > 0, "a dim block should still drop keys");
    }

    #[test]
    fn the_drop_probability_is_capped() {
        // Past the cap, dimming further does not make keys drop any more often.
        // Uncapped this would approach one key missing per column permanently.
        let mut matrix = matrix();
        let total = CENTRE_BLOCK_WIDTH * CENTRE_BLOCK_HEIGHT;
        let (mut at_cap, mut far_below) = (0usize, 0usize);

        for _ in 0..80 {
            matrix.update_dither(BLOCK_DITHER_SECONDS);
            at_cap += total - block_keys_at(&mut matrix, 1.0 - BLOCK_DITHER_MAX_PROBABILITY).len();
            far_below += total - block_keys_at(&mut matrix, 0.05).len();
        }

        assert_eq!(
            at_cap, far_below,
            "below the cap the probability should stop changing"
        );
    }

    #[test]
    fn a_column_never_drops_both_of_its_keys() {
        let mut matrix = matrix();
        let columns = matrix.centre_block_columns();

        // Every nomination, at the brightness where all of them bite.
        for _ in 0..40 {
            matrix.update_dither(BLOCK_DITHER_SECONDS);
            let lit = block_keys_at(&mut matrix, 1.0 - BLOCK_DITHER_MAX_PROBABILITY);
            for column in columns.clone() {
                let in_column = lit.iter().filter(|(_, c)| *c == column).count();
                assert!(
                    in_column >= CENTRE_BLOCK_HEIGHT - 1,
                    "column {column} lost more than one key"
                );
            }
        }
    }

    #[test]
    fn dropping_gets_likelier_as_the_block_dims() {
        // Averaged over many nominations, half brightness should drop far more
        // keys than nearly-full does.
        let mut matrix = matrix();
        let mut at_half = 0usize;
        let mut near_full = 0usize;
        let total = CENTRE_BLOCK_WIDTH * CENTRE_BLOCK_HEIGHT;

        for _ in 0..60 {
            matrix.update_dither(BLOCK_DITHER_SECONDS);
            at_half += total - block_keys_at(&mut matrix, 0.5).len();
            near_full += total - block_keys_at(&mut matrix, 0.95).len();
        }

        assert!(
            at_half > near_full * 3,
            "half brightness dropped {at_half}, near-full dropped {near_full}"
        );
    }

    #[test]
    fn a_nomination_holds_for_its_interval() {
        // Re-rolling per frame would strobe the block.
        let mut matrix = matrix();
        matrix.update_dither(BLOCK_DITHER_SECONDS);
        let held = matrix.dither;

        for _ in 0..20 {
            matrix.update_dither(BLOCK_DITHER_SECONDS / 40.0);
            assert_eq!(matrix.dither, held, "the nomination changed mid-interval");
        }

        matrix.update_dither(BLOCK_DITHER_SECONDS);
        // Redrawn: with four columns picking independently it is vanishingly
        // unlikely to repeat exactly.
        let mut changed = false;
        for _ in 0..8 {
            if matrix.dither != held {
                changed = true;
                break;
            }
            matrix.update_dither(BLOCK_DITHER_SECONDS);
        }
        assert!(changed, "the nomination never changed across intervals");
    }

    // -- The centre block ---------------------------------------------------

    #[test]
    fn the_centre_block_is_eight_keys_straddling_the_axis() {
        let matrix = matrix();
        let mut members = Vec::new();

        for row in 0..MATRIX_ROWS {
            for column in 0..EVEN_COLUMNS {
                if matrix.in_centre_block(row, column) {
                    members.push((row, column));
                }
            }
        }

        assert_eq!(members.len(), CENTRE_BLOCK_WIDTH * CENTRE_BLOCK_HEIGHT);

        // Two rows, one either side of the mirror axis.
        let rows: Vec<usize> = members.iter().map(|(r, _)| *r).collect();
        assert!(rows.iter().all(|r| *r == QUADRANT_ROWS - 1 || *r == QUADRANT_ROWS));
        assert!(rows.contains(&(QUADRANT_ROWS - 1)) && rows.contains(&QUADRANT_ROWS));
    }

    #[test]
    fn the_centre_block_sits_in_the_middle_of_an_even_keyboard() {
        let matrix = matrix();
        let columns = matrix.centre_block_columns();

        // Its centre should land on the keyboard's centre.
        let block_centre = (columns.start + columns.end - 1) as f32 / 2.0;
        let keyboard_centre = (EVEN_COLUMNS - 1) as f32 / 2.0;
        assert!((block_centre - keyboard_centre).abs() < 1e-6);
    }

    #[test]
    fn the_centre_block_fits_inside_a_narrow_keyboard() {
        // Never runs off the end, even if the board is smaller than the block.
        for columns in 1..=MATRIX_COLUMNS {
            let matrix = EqualizerMatrix::new(columns);
            let block = matrix.centre_block_columns();
            assert!(block.end <= columns, "block overruns at width {columns}");
        }
    }

    #[test]
    fn the_block_transform_spans_the_whole_range() {
        // A resting bar should leave the block effectively dark...
        assert!(
            centre_block_brightness(BAR_MIN_FILL) < 0.02,
            "the bar floor should not leave the block glowing: {}",
            centre_block_brightness(BAR_MIN_FILL)
        );
        // ...and a full bar should light it fully.
        assert!((centre_block_brightness(1.0) - 1.0).abs() < 1e-6);
        assert_eq!(centre_block_brightness(0.0), 0.0);
    }

    #[test]
    fn the_block_transform_is_monotonic_and_clamped() {
        let mut previous = -1.0f32;
        for step in 0..=20 {
            let value = centre_block_brightness(step as f32 / 20.0);
            assert!((0.0..=1.0).contains(&value), "left the range: {value}");
            assert!(value >= previous, "dipped at {step}");
            previous = value;
        }
    }

    #[test]
    fn the_centre_block_brightness_tracks_the_bar() {
        let matrix = matrix();
        let loud = levels(0.5, 0.3, matrix.bands);
        let matrix = settled_bar(&loud);
        let frame = matrix.render(&loud);

        let columns = matrix.centre_block_columns();
        let key = frame[QUADRANT_ROWS][columns.start];
        let brightness = key.r.max(key.g).max(key.b) as f32 / 255.0;
        let expected = centre_block_brightness(matrix.bar_fill);

        // Bloom from the bands beside it can only ever add, never take away.
        assert!(
            brightness >= expected - 0.01,
            "block brightness {brightness} should be at least {expected}"
        );
        assert!(
            brightness <= expected.max(BLOOM_STRENGTH) + 0.01,
            "block brightness {brightness} exceeds its own value and any glow"
        );
        // Dimmed, but still a pure colour.
        assert_eq!(key.r.min(key.g).min(key.b), 0);
    }

    #[test]
    fn the_centre_block_matches_the_bar_across_its_whole_range() {
        let matrix = matrix();
        let mut previous = 0u8;

        // Off the boost the block tracks the bar's ordinary range.
        for bass in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let driven = levels(bass, 0.0, matrix.bands);
            let settled = settled_bar(&driven);
            let frame = settled.render(&driven);
            let columns = settled.centre_block_columns();
            // The dither may have dropped this column's nominated key, so take
            // the brightest of the block: at most one key per column goes.
            let brightness = columns
                .clone()
                .map(|column| {
                    let key = frame[QUADRANT_ROWS - 1][column];
                    key.r.max(key.g).max(key.b)
                })
                .max()
                .expect("the block always has columns");

            assert!(
                brightness >= previous,
                "the block should brighten with the bar: {previous} -> {brightness}"
            );
            previous = brightness;
        }

        // And a kick drives it hard, which the ordinary range alone cannot.
        let mut boosted = matrix;
        let loud = levels(1.0, 0.0, boosted.bands);
        boosted.update(&loud, &loud, true, 0.0);
        for _ in 0..3 {
            boosted.update(&loud, &loud, false, TICK);
        }
        let frame = boosted.render(&loud);
        let columns = boosted.centre_block_columns();
        let brightest = columns
            .map(|column| {
                let key = frame[QUADRANT_ROWS - 1][column];
                key.r.max(key.g).max(key.b)
            })
            .max()
            .expect("the block always has columns");
        assert!(
            brightest > 180,
            "a kick should light the block hard: {brightest}"
        );
    }

    #[test]
    fn the_centre_block_goes_black_with_no_audio() {
        let matrix = matrix();
        let quiet = levels(0.0, 0.0, matrix.bands);
        let matrix = settled_bar(&quiet);
        let frame = matrix.render(&quiet);

        for (row, keys) in frame.iter().enumerate() {
            for (column, key) in keys.iter().copied().enumerate() {
                if matrix.in_centre_block(row, column) {
                    assert_eq!(key.r.max(key.g).max(key.b), 0, "block key still lit");
                }
            }
        }
    }

    #[test]
    fn the_centre_block_overrides_the_bands_underneath_it() {
        // Those keys belong to the bass band, which would otherwise draw them at
        // full brightness. The block owns them instead.
        let matrix = matrix();
        let driven = levels(0.35, 0.0, matrix.bands);
        let matrix = settled_bar(&driven);
        let frame = matrix.render(&driven);

        let columns = matrix.centre_block_columns();
        let key = frame[QUADRANT_ROWS][columns.start];
        let brightness = key.r.max(key.g).max(key.b);

        assert!(brightness > 0, "the block should be lit here");
        assert!(
            brightness < 255,
            "the band would have drawn this key at full: {key:?}"
        );
    }

    // -- Note bank ----------------------------------------------------------

    #[test]
    fn the_bank_spans_the_configured_spectrum() {
        let bank = NoteBank::new(TEST_SAMPLE_RATE, 9);

        assert_eq!(bank.bins.len(), 9 * BINS_PER_BAND);
        assert!((bank.lowest_frequency() - BASS_BAND_MIN_HZ).abs() < 0.01);
        assert!(bank.highest_frequency() < SPECTRUM_MAX_HZ);
        assert!(bank.highest_frequency() > SPECTRUM_MAX_HZ * 0.8);
        // Low bins integrate over a longer window than high bins.
        assert!(bank.bins[0].window > bank.bins[bank.bins.len() - 1].window);
    }

    #[test]
    fn the_bass_band_is_pinned_whatever_the_keyboard_width() {
        // What counts as bass must not move with the hardware.
        for bands in [5usize, 7, 9, 10] {
            let bank = NoteBank::new(TEST_SAMPLE_RATE, bands);

            assert!((bank.bins[0].frequency - BASS_BAND_MIN_HZ).abs() < 0.01);
            // The last bass bin sits just under the cutoff, and the first band
            // above it starts exactly on it.
            assert!(bank.bins[BINS_PER_BAND - 1].frequency < BASS_BAND_MAX_HZ);
            assert!(
                (bank.bins[BINS_PER_BAND].frequency - BASS_BAND_MAX_HZ).abs() < 0.01,
                "band 1 should start at the cutoff at {bands} bands"
            );
        }
    }

#[test]
    fn a_quiet_high_band_still_reaches_full_scale() {
        // The point of per-band AGC. Under one global follower the loud bass
        // would set the scale and this treble tone would never leave the floor.
        let mut bank = NoteBank::new(TEST_SAMPLE_RATE, 9);
        feed(&mut bank, &[(60.0, 0.5), (5000.0, 0.02)], 60);

        let levels = bank.levels();
        let treble = levels.iter().skip(6).copied().fold(0.0f32, f32::max);

        assert!(levels[0] > 0.5, "the bass band should be lit: {levels:?}");
        assert!(
            treble > 0.5,
            "a quiet treble tone should still fill its own band: {levels:?}"
        );
    }

    #[test]
    fn levels_settle_back_to_nothing_in_silence() {
        let mut bank = NoteBank::new(TEST_SAMPLE_RATE, 9);
        feed(&mut bank, &[(60.0, 0.5)], 40);
        assert!(bank.levels()[0] > 0.5);

        feed(&mut bank, &[(60.0, 0.0)], 400);
        for (band, level) in bank.levels().iter().enumerate() {
            assert!(*level < 0.1, "band {band} still reads {level} in silence");
        }
    }

    // -- Frame delivery -----------------------------------------------------

    #[test]
    fn stopping_the_effect_disowns_queued_frames() {
        // Sleep and shutdown reach the device worker on its urgent channel and
        // black the keyboard straight away, while frames already queued on the
        // normal channel are still waiting. Those must not be drawn afterwards,
        // or the keyboard lights back up as the machine suspends.
        AudioBloomEffect::stop();
        assert!(
            !AudioBloomEffect::is_active(),
            "a stopped effect must not own the keyboard"
        );

        // And it is cleared before the thread is joined, not after, so there is
        // no window where a late frame still counts as live.
        AudioBloomEffect::stop();
        assert!(!AudioBloomEffect::is_active());
    }



    #[test]
    fn only_changed_rows_are_sent() {
        let frame = vec![vec![ThemeColor::new(1, 2, 3); EVEN_COLUMNS]; MATRIX_ROWS];

        // Nothing has been sent yet, so every row is new.
        assert_eq!(changed_rows(&frame, &[], false).len(), MATRIX_ROWS);
        // An identical frame costs nothing.
        assert!(changed_rows(&frame, &frame, false).is_empty());

        let mut moved = frame.clone();
        moved[4][2] = ThemeColor::new(9, 9, 9);
        let changed = changed_rows(&moved, &frame, false);
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].0, 4);
    }

    #[test]
    fn a_full_refresh_resends_everything() {
        let frame = vec![vec![ThemeColor::new(1, 2, 3); EVEN_COLUMNS]; MATRIX_ROWS];

        assert_eq!(changed_rows(&frame, &frame, true).len(), MATRIX_ROWS);
    }

    #[test]
    fn sustained_voices_produce_no_kicks() {
        // The bass band now reaches up into vocal territory on purpose, so the
        // band edge no longer keeps voices out - the detector has to.
        for hz in [130.0f64, 180.0, 220.0] {
            assert_eq!(
                hits_in_final_segment(&[(hz, 0.5, 60), (hz, 0.5, 120)]),
                0,
                "{hz} Hz held should not register as a kick"
            );
        }
    }

    #[test]
    fn a_real_kick_still_reaches_full_scale() {
        // The floor must not cost genuine bass its range.
        let mut bank = NoteBank::new(TEST_SAMPLE_RATE, 9);
        feed(&mut bank, &[(50.0, 0.5)], 40);

        assert!(
            bank.levels()[0] > 0.8,
            "a kick should still reach full scale: {}",
            bank.levels()[0]
        );
    }

    #[test]
    fn a_quiet_kick_still_registers() {
        // A fifth of the amplitude still sits above the bass floor, so quiet
        // tracks are not shut out by it.
        let mut bank = NoteBank::new(TEST_SAMPLE_RATE, 9);
        feed(&mut bank, &[(50.0, 0.1)], 40);

        assert!(
            bank.levels()[0] > 0.8,
            "a quiet kick should still read strongly: {}",
            bank.levels()[0]
        );
    }


}
