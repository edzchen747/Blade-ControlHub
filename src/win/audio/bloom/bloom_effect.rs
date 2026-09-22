use crate::config::ThemeColor;
use crate::razer::device_handle::DeviceHandle;
use crate::razer::handlers::keyboard::{MATRIX_COLUMNS, MATRIX_ROWS};
use crate::win::display::ambient::ensure_process_mta;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing::{info, warn};
use crate::win::audio::{
    AudioType, DefaultEndpointWatcher, default_endpoint, default_endpoint_id, endpoint_info,
    take_com_string,
};
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient,
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoTaskMemFree};

// -- Timing ------------------------------------------------------------------

/// Target frame rate. Everything is driven by the measured `dt`, so if the HID
/// path cannot keep up the animation slows rather than desyncing.
const FPS: f64 = 60.0;
/// The tick length the smoothing rates are expressed against, so changing `FPS`
/// does not change how fast anything settles.
const REFERENCE_TICK: f32 = 1.0 / 60.0;
/// Rows are re-sent in full this often even if unchanged, so nothing else can
/// leave stale colour on the keyboard.
const FULL_REFRESH_FRAMES: u64 = 30;
/// How long to wait before retrying after the capture endpoint drops out.
const BLOOM_RECOVERY_DELAY: Duration = Duration::from_secs(3);
/// The offset assumed for a Bluetooth endpoint that will not report its own.
///
/// Bluetooth playback reaches the ear well after it passes the render stream
/// this captures, so without compensation the keyboard reacts before the sound
/// arrives. When the endpoint reports a figure that is used instead; this only
/// covers the case where Windows exposes nothing and the alternative is no
/// correction at all.
///
/// Applied to Bluetooth alone, because a wired endpoint reporting nothing
/// almost certainly has nothing to report, while a Bluetooth one reporting
/// nothing is a gap in what the API surfaces rather than an absence of delay.
const BLUETOOTH_FALLBACK_OFFSET_MS: f64 = 50.0;
/// How often to re-read the default endpoint when there is no subscription.
///
/// Only the fallback. Normally Windows reports the change through
/// `DefaultEndpointWatcher` and this never runs; it exists so that a failure to
/// register the callback degrades to a slower reaction rather than to the
/// effect being stuck on one device again.
const ENDPOINT_CHECK_INTERVAL: Duration = Duration::from_secs(1);

// -- Matrix geometry ---------------------------------------------------------

/// Rows making up the mirrored visualiser, leaving row 6 for the bass bar.
const VISUALIZER_ROWS: usize = 6;
/// Rows in each quadrant. The mirror axis sits between rows 2 and 3.
const QUADRANT_ROWS: usize = VISUALIZER_ROWS / 2;
/// The row carrying the single left-to-right bass bar.
const BAR_ROW: usize = 6;

// -- Bass response -----------------------------------------------------------

/// How much extra height the bass grants a band at full bass, as a multiple of
/// that band's own level.
///
/// The centre band is tripled, so a loud band there fills its quadrant outright.
/// The outermost is only doubled, with a linear ramp between: the bass should
/// push hardest where it is actually drawn and have progressively less say over
/// the far treble, rather than inflating the whole board evenly.
const BASS_BOOST_CENTRE: f32 = (QUADRANT_ROWS - 1) as f32;
const BASS_BOOST_OUTER: f32 = 1.0;

/// How long a boost lasts once it fires, fading linearly to nothing across it.
///
/// The boost is a punch, not a state. Held bass would otherwise park the whole
/// board at full height for as long as the note lasted, which reads as a stuck
/// visualiser rather than a reactive one.
const BASS_BOOST_SECONDS: f32 = 0.5;
/// How loud the bass band has to be for a detected kick to fire the boost.
///
/// Detection is deliberately relative - flux is normalised against its own
/// recent median so one threshold works in a quiet verse and a loud chorus
/// alike. That is right for *finding* a kick and wrong for deciding whether it
/// deserves to throw the whole board to full height: a faint tap in a near-silent
/// passage clears the same bar as the drop it was meant to catch.
///
/// This is the absolute check the relative one cannot make. A hit still
/// registers; it just does not get the boost unless there is real weight behind
/// it.
///
/// Measured against the *raw* band level rather than the smoothed one, so a
/// sharp kick is judged on its actual height instead of on an envelope that has
/// not caught up with it yet.
///
/// One honest caveat: the bass band is AGC-normalised, so this is "quiet
/// relative to the band's recent peak" rather than quiet in absolute terms. The
/// `BASS_AGC_FLOOR` on the peak follower stops that stretch running away in
/// silence, which is what makes the test meaningful at all. Gating on absolute
/// magnitude would mean plumbing `NoteBank::power` through to here.
const BASS_BOOST_MIN_LEVEL: f32 = 0.30;
/// How long the bands flash white when a boost fires.
///
/// Drawn by desaturating rather than by overwriting the colour, so a key's
/// brightness is untouched and one that is unlit stays unlit. The flash marks
/// the bands only; the bar keeps its hue throughout, because it reads as a
/// meter and whiting it out would swamp the length cue it exists to give.
///
/// It fades across the window instead of cutting out at the end, for the same
/// reason `BASS_BOOST_SECONDS` does: a hard edge snaps the whole board at once.
const BAND_FLASH_SECONDS: f32 = 0.10;

/// The least perceived brightness any hue in the cycle may be drawn at.
///
/// HSV treats every hue as equally bright at a given value, and the eye does
/// not. Full-value blue is `rgb(0, 0, 255)`, which carries about 7% of the
/// luminance of full-value green - so a cycle that walks the hue circle at
/// constant value visibly sinks into darkness through the blues and purples and
/// comes back out the other side.
///
/// The fix is to desaturate rather than to skip. Jumping over that arc would put
/// a visible seam in a cycle that takes `RAINBOW_PERIOD_SECONDS` to come round,
/// and it would lose the blues altogether; mixing white into them instead keeps
/// the whole circle and turns the dark navy into a bright blue and the deep
/// purple into a bright violet. Hues already above the floor - the greens,
/// cyans, yellows - are left at full saturation and do not change at all.
const MIN_HUE_LUMINANCE: f32 = 0.32;
// Kick detection: spectral flux over a dedicated fixed-window bin bank.
//
// Every constant here was fitted against real captures rather than synthetic
// tones, using how tightly the resulting hits lock to the track's beat grid as
// the objective. Isolated sine tests are actively misleading for this - they
// have no harmonics, no percussion and nothing else playing at once, which is
// exactly what the detector has to cope with, and they have repeatedly
// predicted the opposite of what real audio does.

/// Samples in the onset analysis window, the same for every bin.
///
/// 2048 at 48 kHz is ~43 ms and resolves ~23 Hz. Shorter would track the
/// transient more sharply but cannot separate the low bins at all: 1024 samples
/// resolves only ~47 Hz, so every bin under 200 Hz would see the same thing.
const FLUX_WINDOW_SAMPLES: usize = 2048;
/// Samples between one flux measurement and the next.
///
/// The analysis rate is deliberately decoupled from the frame rate. Driven off
/// rendered frames it ran at ~30 Hz with 21% window overlap, so the flux was
/// sampled every 34 ms while a kick attack lasts 5-10 ms - the transient was
/// being smeared across a single measurement and detection could not be
/// consistent however the thresholds were set. 512 samples is ~10.7 ms, 94 Hz
/// and 75% overlap, which is normal practice for onset detection.
const FLUX_HOP_SAMPLES: usize = 512;

/// The two low ranges the kick detector listens over, analysed separately.
///
/// Low bands only, so this stays a kick detector. Summing the whole spectrum
/// locks to the beat better still (0.70 against 0.55 on a capture) but fires on
/// snares and hats too, at which point the boost stops meaning "bass".
///
/// They are split because one flux figure spanning 40-500 Hz hides the thing it
/// is looking for. A sub-bass drop and a kick attack land in different parts of
/// the range, and a band wide enough to hold both dilutes either one into the
/// other's noise - a dense mix can hold the wide band's total almost flat while
/// the sub region moves sharply underneath it. Each band carries its own flux,
/// its own median history and its own threshold test.
///
/// Only *one* of them emits at a time - see `BAND_PRESENCE_SECONDS`. Firing on
/// both makes a track that uses both read as too busy to hold a beat, so the
/// detector picks the deeper band when the song actually has one.
///
/// One honest limitation: the sub band's bottom sits below what the analysis
/// window can resolve. 2048 samples at 48 kHz is 43 ms, shorter than a single
/// period of 20 Hz, so the lowest bins here cannot be separated from one
/// another and effectively report the same thing. They are kept because they do
/// still respond to energy arriving in the region, and separating them properly
/// would need a longer window - which would cost the transient timing that
/// matters more than sub-bass pitch resolution ever could.
const FLUX_SUB_MIN_HZ: f64 = 20.0;
const FLUX_SUB_MAX_HZ: f64 = 60.0;
const FLUX_SUB_BINS: usize = 8;
const FLUX_KICK_MIN_HZ: f64 = 60.0;
const FLUX_KICK_MAX_HZ: f64 = 150.0;
const FLUX_KICK_BINS: usize = 12;

/// How long the detector looks back when deciding which band a song lives in.
///
/// Songs differ in where their low end sits: some carry a real sub-bass, some
/// have nothing under ~60 Hz and put the whole kick in the punch range. Firing
/// on both is wrong for either, so each band's recent energy is tracked and the
/// *deepest* one that is genuinely present gets the floor to itself.
///
/// Presence is judged on energy rather than on how many onsets a band produced.
/// The question being asked is "does this song have deep bass", which is about
/// content, not about where the beat happens to land - and the gap fallback
/// below covers the case where those two differ.
const BAND_PRESENCE_SECONDS: f32 = 5.0;
/// The share of the loudest band's energy needed to take the floor, and the
/// smaller share needed to keep it once held.
///
/// A ratio rather than an absolute level because music is roughly pink: the
/// 20-60 Hz region is usually louder in absolute terms than 60-150 Hz, so an
/// absolute test would select sub on very nearly everything. What separates a
/// sub-heavy mix from a thin one is how the two regions compare - typically
/// 0.1-0.3 with no deliberate sub against 0.8-2.0 with one.
///
/// The two figures differ on purpose. A challenger has to reach `ENGAGE` to take
/// over while the incumbent only has to hold `RELEASE`, which is a Schmitt
/// trigger: without the gap, a track sitting near the boundary would swap bands
/// every few seconds.
const BAND_PRESENCE_ENGAGE: f32 = 0.60;
const BAND_PRESENCE_RELEASE: f32 = 0.35;
/// Below this the selection is held rather than re-judged.
///
/// Without it a quiet passage would re-decide the band on nothing but noise.
///
/// Measured, after an estimate of 0.05 turned out to be an order of magnitude
/// too high: on a real capture the two low bands sat at 0.030 and 0.032, so the
/// floor was above every reading and `update_selection` returned early on every
/// hop. Selection never ran at all and the band stayed on whatever it was
/// initialised to, which looked exactly like a band being chosen and held.
///
/// The estimate was wrong because it reasoned about the compressed scale of a
/// whole band (`ln(1+100m)` over a strong signal) rather than the *per-bin mean*
/// this actually averages, which is far smaller. 0.005 leaves roughly a factor
/// of six below the measured level while still sitting well above silence.
const BAND_PRESENCE_FLOOR: f32 = 0.005;
/// How many beats the selected band may stay silent before the others are let
/// through, and the interval assumed before a beat has been measured.
///
/// Energy presence can pick a band the beat does not actually land in - a track
/// with a sustained sub drone under punch-range drums is exactly that case. This
/// is what stops the visualiser going dark through it, and through breakdowns
/// where the sub simply drops out for a few bars.
const BAND_FALLBACK_GAPS: f32 = 1.5;
const BAND_FALLBACK_DEFAULT_SECONDS: f32 = 1.0;

/// The mid range the low flux is compared against, to reject voices.
const FLUX_MID_MIN_HZ: f64 = 500.0;
const FLUX_MID_MAX_HZ: f64 = 4000.0;
const FLUX_MID_BINS: usize = 12;
/// A low band's flux must be at least this multiple of the mid flux for a hit
/// to count - compared per bin, not as raw sums.
///
/// Per bin because the bands no longer hold equal numbers of them. Flux is a sum
/// over bins, so comparing sums directly would make a narrow band look weak for
/// no reason but its width, and the sub band would never clear the test. 0.2 per
/// bin is the same comparison the fitted 0.4 made on sums, which ran 24 low bins
/// against 12 mid ones.
const FLUX_DOMINANCE: f32 = 0.2;

/// Compression applied to each bin magnitude before differencing.
///
/// This is what makes the flux a measure of *relative* change rather than of
/// absolute loudness. Measured offline, 100 and 500 give identical results, so
/// the value is not sensitive.
const FLUX_COMPRESSION: f32 = 100.0;

/// The window the flux is normalised over.
///
/// The deviation floor matters more than it looks. Dividing by a median
/// deviation turns a near-constant ripple into a large figure when that
/// deviation is tiny, and a sustained tone is exactly that: its Goertzel
/// magnitude wobbles as the window slides over it. Without a floor on the scale
/// of a real onset, a held note fires repeatedly.
const FLUX_NORMALISE_SECONDS: f32 = 1.0;
const FLUX_DEVIATION_FLOOR: f32 = 0.05;
/// How many deviations above the median an onset has to reach.
///
/// A fixed figure, which is the point of normalising: the detection function is
/// unitless, so one threshold holds across quiet and loud passages alike.
///
/// Swept against a capture: 4.0 gave 18 hits where the signal held 16 onsets,
/// 3.0 gave 29 and 5.0 gave 12. Erring slightly high on the count, since the
/// complaint this replaces was of beats being missed.
const FLUX_THRESHOLD: f32 = 4.0;
/// The shortest gap between two hits.
///
/// 150 ms, which puts the ceiling at 400 BPM - far above anything a drummer
/// plays, so it cannot cost a real beat. Counting onsets straight off a capture
/// with no refractory gave 22 in 20 s with the gaps clustered at 0.6-0.7 s, so
/// there is a wide margin either side of this figure.
///
/// It guards against the sub-bass tail of a kick re-triggering behind its own
/// attack, which now matters more than it did: two bands fire independently and
/// the sub band is exactly the one that rings on after the strike.
const BASS_REFRACTORY_SECONDS: f32 = 0.15;

// -- Frequency bands ---------------------------------------------------------

/// The bass band is pinned to an absolute range rather than taking its share of
/// the log-spaced spectrum.
///
/// Pinning it stops the definition of "bass" shifting with the keyboard width,
/// which the even log spacing would otherwise do.
///
/// The ceiling is set by where kick energy actually is, measured off a real
/// track: autocorrelating each band against the beat put the strongest pulse at
/// 144-260 Hz and the *weakest* in 40-80 Hz, which carried only a 2x dynamic
/// range and almost no transient to detect. A band that stops below ~150 Hz
/// cannot see the kick it is supposed to be triggering on.
///
/// The cost is that male vocal fundamentals (85-180 Hz) now fall inside the
/// band, so the rise and dominance tests in `detect_bass_hit` are what keep
/// speech from registering as a kick - the band edge no longer does it.
///
/// The floor sits below what laptop speakers can reproduce, on purpose. This is
/// WASAPI *loopback*: it reads the render stream before it ever reaches the
/// speakers, so their response is irrelevant to what gets measured, and kick
/// fundamentals live down here whatever the drivers then do with them.
const BASS_BAND_MIN_HZ: f64 = 40.0;
const BASS_BAND_MAX_HZ: f64 = 200.0;
/// Every band above the bass is log-spaced from `BASS_BAND_MAX_HZ` up to here.
const SPECTRUM_MAX_HZ: f64 = 16_000.0;
/// Goertzel filters per band. A band takes the loudest of its four, which stays
/// reactive where averaging an octave-wide band would not.
const BINS_PER_BAND: usize = 4;
/// Each Goertzel window spans this many periods of its own centre frequency, so
/// low bins integrate over a long window and high bins over a short one.
const PERIODS_PER_WINDOW: f64 = 4.0;
const MIN_WINDOW: usize = 64;
const MAX_WINDOW: usize = 4096;

/// How far below a band's own peak its level reaches zero.
///
/// Levels are drawn on a dB scale: linear magnitude crowds everything quiet into
/// the bottom of the range, which is why the bands read as either saturated or
/// dark with little in between.
///
/// Narrow on purpose. Measured over a capture, band 0's content spans only
/// ~17 dB while band 9's spans ~46 dB, so a window wide enough for the treble
/// leaves the bass sitting in the top third of it and never going dark - at
/// 45 dB the bass band read p05 0.83, p50 0.95 and was lit 100% of the time.
/// At 12 dB the same band reads 0.36 / 0.82 / 0.99 and actually moves.
///
/// The cost is that the widest-ranging bands clip at the bottom, which is the
/// right trade for a visualiser: a band that never leaves the top of its range
/// shows nothing at all.
const DISPLAY_RANGE_DB: f32 = 12.0;
/// Keeps the dB conversion away from log(0) during silence.
const DB_EPSILON: f32 = 1e-6;
/// Per-band peak-follower rates. Fast attack so a loud passage stops clipping
/// quickly, slow release so a quiet one is not immediately pumped back up.
///
/// Per-band rather than global: one follower across the whole spectrum lets the
/// bass swamp everything, and the upper bands then never leave the floor.
const AGC_ATTACK: f32 = 0.35;
const AGC_RELEASE: f32 = 0.004;
/// Keeps a follower from dividing by roughly zero during silence.
const AGC_FLOOR: f32 = 0.0008;
/// The bass band's follower cannot scale below this, unlike the rest.
///
/// Per-band AGC stretches whatever a band sees to full range. Through a
/// vocals-only passage band 0 has nothing but leakage to measure, so it adapts
/// down to it — around 0.001 against a real kick's 0.24 — and the next loud
/// syllable then reads as a full-scale kick and fires the bass boost. Speech has
/// no business driving the bass mechanics, and no band edge can prevent this on
/// its own: any residual leakage becomes full scale once the reference collapses
/// to it.
///
/// Set well above the leakage a voice puts into 40-80 Hz and well below what a
/// real kick produces, so genuine bass still reaches full scale while quiet
/// passages stay quiet.
const BASS_AGC_FLOOR: f32 = 0.03;

/// Displayed level smoothing, as a fraction of the gap closed per reference
/// tick. Snappy on the way up, unhurried on the way down.
const LEVEL_ATTACK: f32 = 0.55;
const LEVEL_RELEASE: f32 = 0.10;

// -- Colour ------------------------------------------------------------------

/// Seconds for the single shared colour to travel the whole hue circle.
const RAINBOW_PERIOD_SECONDS: f32 = 45.0;

// -- Dormant mode ------------------------------------------------------------

/// How long the keyboard takes to cross between the visualiser and the dormant
/// colour cycle, in either direction.
///
/// With nothing playing the visualiser is a black keyboard, which reads as the
/// effect having stopped rather than as the music having. Dormant mode lights
/// the whole board in the same cycling colour the bands are drawn in, so an
/// idle keyboard still shows the effect is running.
///
/// The wait before it starts is `BAR_SILENCE_DELAY_SECONDS`, reusing the
/// existing gap-versus-silence judgement rather than making a second one: the
/// board should not sink into the cycle over the beat of rest between tracks.
const DORMANT_FADE_SECONDS: f32 = 0.5;
/// How long the board is held black between the dormant fade-out and the
/// visualiser resuming.
///
/// The visualiser keeps running underneath throughout, so it comes back mid-
/// track rather than from a standing start. The pause is there to separate the
/// two states: crossing straight from a full board into a full-height first
/// frame reads as a glitch, while a beat of black reads as a hand-off.
const DORMANT_WAKE_BLACK_SECONDS: f32 = 0.5;

// -- Bass bar (row 6) --------------------------------------------------------

/// The bar never drops below this much of the row *while there is audio*.
/// Silence takes it to nothing instead, so the keyboard goes properly dark.
const BAR_MIN_FILL: f32 = 0.10;
/// How long the audio has to stay quiet before it counts as silence.
///
/// Without this the bar collapses to nothing on any momentary gap, then has to
/// climb back, which reads as a flicker rather than as the system going quiet.
/// The beat of rest between tracks, a breath in a quiet passage and a single
/// dropped buffer all look identical to silence at the moment they arrive.
///
/// Through the delay the bar holds at `BAR_MIN_FILL`, the same floor it keeps
/// while something is playing, so a brief gap is invisible. Only once the
/// quiet outlasts this does the target drop to zero and the row go dark.
const BAR_SILENCE_DELAY_SECONDS: f32 = 0.5;
/// How much of the row the bass band alone commands, from `BAR_MIN_FILL` up.
const BAR_BASS_SPAN: f32 = 0.80;
/// The share of the row any ordinary band commands.
///
/// Off the boost the bass is simply one band among the rest, competing for the
/// bar on equal terms, and the bar tracks whichever is loudest at this weight.
/// That keeps it low and mobile instead of pinned near the top, which is what it
/// did while the bass held the whole span to itself.
const BAR_OTHER_SHARE: f32 = 1.0 / 3.0;

/// The bar runs its own envelope over the *unsmoothed* band levels.
///
/// Releasing faster than `LEVEL_RELEASE` on top of the already-smoothed levels
/// would achieve nothing: the bar would simply catch a slowly-falling target and
/// then fall at that target's rate. Driving it from the raw measurement is what
/// actually lets it drop faster than the quadrants.
const BAR_ATTACK: f32 = 0.70;
const BAR_RELEASE: f32 = 0.30;
/// How much extra the bar's bass term gets while a boost window is open: +200%
/// as it fires, fading out on the same envelope the bands use.
///
/// Same reasoning as the bands: a held note would otherwise park the bar near
/// the end of the row for as long as it lasted. Boosting only on the spike makes
/// the bar punch out on each kick and settle back between them.
const BAR_BASS_BOOST: f32 = 2.0;

// -- Centre block ------------------------------------------------------------

/// A block of keys at the very middle of the keyboard that mirrors the bass bar,
/// but expressed as brightness rather than as a fill.
///
/// This is the one place the output is not binary. Everywhere else a partly-lit
/// LED reads as mud because the keys sit so far apart, but these eight are
/// adjacent and large enough to read as a single object that dims and swells.
const CENTRE_BLOCK_WIDTH: usize = 4;
/// Must be even: the block straddles the mirror axis, half above and half below.
const CENTRE_BLOCK_HEIGHT: usize = 2;

/// Stretches the bar's fill before the block draws it, as `v * SCALE - OFFSET`.
///
/// The bar holds `BAR_MIN_FILL` while anything at all is playing, which would
/// otherwise leave the block sitting at a permanent tenth brightness. This drops
/// that floor to effectively nothing while a full bar still reaches full
/// brightness, so the block uses the whole 0..100% range.
const CENTRE_BLOCK_SCALE: f32 = 1.1;
const CENTRE_BLOCK_OFFSET: f32 = 0.1;

/// The block dissolves as it dims rather than simply fading.
///
/// Each column of the block nominates one of its keys to drop out, with
/// probability `1 - brightness`: untouched at full, an even chance by the time
/// it is half lit. The block breaks up as it falls instead of the whole thing
/// sliding down together, which reads as motion on eight keys that a uniform
/// fade does not.
///
/// The probability is capped here rather than being allowed to run on to 1.0.
/// Uncapped, a nearly-dark block would have a key missing from every column
/// almost permanently, which stops reading as a dissolve and just looks like
/// four keys. Capped, it keeps breaking up all the way down.
const BLOCK_DITHER_MAX_PROBABILITY: f32 = 0.5;
/// How long a nomination holds before it is drawn again.
///
/// Re-rolling per frame would strobe; a second is long enough to read as a
/// pattern and short enough that it keeps changing.
const BLOCK_DITHER_SECONDS: f32 = 1.0;

// -- Bloom -------------------------------------------------------------------

/// How brightly a lit key's neighbours glow **at full bass**, for a key in the
/// bass band at the centre of the keyboard. The bass level scales it from
/// nothing up to this.
///
/// Spill is taken as the brightest neighbour rather than the sum, so a key
/// surrounded by lit keys glows at this level and no more. Summing would let the
/// glow saturate and spread outward at full strength, which is a flood rather
/// than a bloom.
const BLOOM_STRENGTH: f32 = 0.5;
/// The same ceiling for the outermost, highest-frequency band, with a linear
/// ramp between the two.
///
/// The glow is a bass effect, so it is strongest where the bass is drawn and
/// fades as the bands run out toward the edges of the keyboard. Without this the
/// treble edges bloom exactly as hard as the centre, which reads as the whole
/// board swelling rather than the bass pushing outward from the middle.
const BLOOM_STRENGTH_OUTER: f32 = 0.2;
/// Whether diagonal neighbours glow too.
///
/// Off, for two reasons: the columns are staggered, so a diagonal neighbour is
/// not reliably the key sitting next to it, and a full 3x3 glow around every lit
/// key merges neighbouring bands into one blob.
const BLOOM_DIAGONALS: bool = false;

// -- Lifecycle ---------------------------------------------------------------

/// Frames handed to the device worker that it has not written yet.
///
/// The worker's queue is unbounded, so an effect that renders faster than the
/// HID path can write would grow it without limit and the keyboard would fall
/// steadily further behind the music. Capping the frames in flight makes the
/// effect drop frames instead of queueing them, which keeps it in sync.
static FRAMES_IN_FLIGHT: AtomicU32 = AtomicU32::new(0);
/// How many frames may be queued before the effect starts skipping.
const MAX_FRAMES_IN_FLIGHT: u32 = 2;

/// Whether the effect currently owns the keyboard.
///
/// Frames are queued on the device worker's normal channel, but sleep and
/// shutdown arrive on its *urgent* one and are handled first. Without this gate
/// the frames still sitting in the queue are painted straight after the sleep
/// blackout, relighting the keyboard as the machine suspends. Clearing it before
/// the join means anything already queued is dropped rather than drawn.
static EFFECT_ACTIVE: AtomicBool = AtomicBool::new(false);

pub static THREAD_GENERATION: AtomicU32 = AtomicU32::new(0);
static BLOOM_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

pub struct AudioBloomEffect {}

impl AudioBloomEffect {
    /// `columns` is the probed width of the top row, so the strip is scaled to
    /// the LEDs that exist rather than to the protocol maximum.
    pub fn start(device_handle: DeviceHandle, columns: usize) {
        Self::stop();
        FRAMES_IN_FLIGHT.store(0, Ordering::SeqCst);
        EFFECT_ACTIVE.store(true, Ordering::SeqCst);
        let current_generation = THREAD_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

        match thread::Builder::new()
            .name("blade-audio-bloom".to_string())
            .spawn(move || run_bloom_loop(device_handle, columns, current_generation))
        {
            Ok(handle) => {
                *bloom_thread() = Some(handle);
            }
            Err(error) => {
                warn!(%error, "Failed to start audio bloom effect thread");
            }
        }
    }

    pub fn stop() {
        // Before the join, so frames queued while the thread winds down are
        // discarded rather than painted after a sleep blackout.
        EFFECT_ACTIVE.store(false, Ordering::SeqCst);
        THREAD_GENERATION.fetch_add(1, Ordering::SeqCst);
        join_bloom_thread();
    }

    /// Whether queued frames should still be drawn.
    pub fn is_active() -> bool {
        EFFECT_ACTIVE.load(Ordering::SeqCst)
    }

    /// Called by the device worker once a frame has been written.
    pub fn frame_written() {
        let _ = FRAMES_IN_FLIGHT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |pending| {
            pending.checked_sub(1)
        });
    }
}

fn bloom_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    BLOOM_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_bloom_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = bloom_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current audio bloom thread during shutdown");
        return;
    }

    if handle.join().is_err() {
        warn!("Audio bloom effect thread panicked during shutdown");
    }
}

fn run_bloom_loop(device_handle: DeviceHandle, columns: usize, current_generation: u32) {
    let columns = columns.clamp(1, MATRIX_COLUMNS);

    // Unlike the ambient effect this thread is never demoted. Ambient yields
    // because it competes with the compositor for the GPU; this one is CPU-only
    // and audio-timed, and starving it shows up directly as stutter.
    ensure_process_mta();

    let mut capture = match LoopbackCapture::open() {
        Ok(capture) => capture,
        Err(error) => {
            warn!(%error, "Audio bloom disabled because loopback capture could not start");
            schedule_bloom_recovery(device_handle, columns, current_generation);
            return;
        }
    };

    let mut matrix = EqualizerMatrix::new(columns);
    let mut bank = NoteBank::new(capture.sample_rate() as f64, matrix.bands);

    info!(
        sample_rate = capture.sample_rate(),
        channels = capture.channels(),
        columns,
        bands = matrix.bands,
        lowest_hz = bank.lowest_frequency(),
        highest_hz = bank.highest_frequency(),
        "Audio bloom capturing the default render endpoint"
    );

    let mut previous: Vec<Vec<ThemeColor>> = Vec::new();
    let mut frames: u64 = 0;
    let mut last_tick = Instant::now();
    let mut last_endpoint_check = Instant::now();

    // Windows tells us when the default output moves, rather than the loop
    // asking once a second forever. Registered before the first tick so a
    // change arriving during startup is still caught.
    let endpoint_watcher = match DefaultEndpointWatcher::new(AudioType::Speakers) {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            warn!(%error, "Could not subscribe to output device changes; falling back to polling");
            None
        }
    };

    while THREAD_GENERATION.load(Ordering::SeqCst) == current_generation {
        let start = Instant::now();

        let endpoint_may_have_moved = match &endpoint_watcher {
            Some(watcher) => watcher.take_change(),
            None => start.duration_since(last_endpoint_check) >= ENDPOINT_CHECK_INTERVAL,
        };
        if endpoint_may_have_moved {
            last_endpoint_check = start;
            follow_default_endpoint(&mut capture, &mut bank, matrix.bands);
        }

        if let Err(error) = capture.pump(bank.samples_mut()) {
            warn!(%error, "Audio bloom loopback capture failed");
            schedule_bloom_recovery(device_handle, columns, current_generation);
            return;
        }

        let now = Instant::now();
        // A scheduling hiccup must not let one tick jump the animation.
        let dt = (now - last_tick).as_secs_f32().clamp(0.0, 0.25);
        last_tick = now;

        bank.analyze(dt);
        matrix.update(bank.levels(), bank.raw_levels(), bank.bass_hit(), dt);

        // Keep analysing even when a frame is dropped, so the visuals stay in
        // step with the music rather than running in slow motion.
        if FRAMES_IN_FLIGHT.load(Ordering::SeqCst) < MAX_FRAMES_IN_FLIGHT {
            let frame = matrix.render(bank.levels());
            let full_refresh = frames.is_multiple_of(FULL_REFRESH_FRAMES);
            let changed = changed_rows(&frame, &previous, full_refresh);
            if !changed.is_empty() {
                FRAMES_IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
                device_handle.set_key_rows(changed, full_refresh);
            }
            previous = frame;
            frames += 1;
        }

        sleep_until_next_frame(start);
    }
}

/// Moves the capture onto the default output device when it changes.
///
/// Without this the effect works only on whichever device was default when it
/// started. Changing output leaves the original endpoint valid but idle, so the
/// capture keeps succeeding and returns nothing, which is indistinguishable
/// from a silent system - the keyboard simply goes dark until the old device is
/// made default again.
///
/// The id is still compared even when a notification prompted the call. The
/// callback fires for changes this capture does not care about, and comparing
/// costs one cheap COM call against needlessly tearing down a working stream.
///
/// A failure to reopen is left for the next signal rather than torn down. The
/// new endpoint is often busy for a moment during a switch, and a capture that
/// works is worth more than a tidy error path.
fn follow_default_endpoint(capture: &mut LoopbackCapture, bank: &mut NoteBank, bands: usize) {
    if !capture.endpoint_changed() {
        return;
    }

    let replacement = match LoopbackCapture::open() {
        Ok(replacement) => replacement,
        Err(error) => {
            warn!(%error, "Output device changed but the new endpoint could not be opened");
            return;
        }
    };

    let rate_changed = replacement.sample_rate() != capture.sample_rate();
    info!(
        sample_rate = replacement.sample_rate(),
        channels = replacement.channels(),
        rate_changed,
        "Audio bloom following the output device change"
    );
    *capture = replacement;

    // Every filter coefficient is derived from the sample rate, so a device at
    // a different rate needs the bank rebuilt rather than reused. Same-rate
    // switches keep theirs, which preserves the AGC and the band selection
    // instead of making the visualiser re-learn the track it is already on.
    if rate_changed {
        *bank = NoteBank::new(capture.sample_rate() as f64, bands);
    }
}

/// Picks out the rows whose colours actually moved since the last frame.
///
/// A full matrix is seven HID writes, which is a lot to spend every frame at 60
/// fps. During quiet passages most rows sit still, so sending only what changed
/// keeps the average cost far below the worst case.
fn changed_rows(
    frame: &[Vec<ThemeColor>],
    previous: &[Vec<ThemeColor>],
    full_refresh: bool,
) -> Vec<(u8, Vec<ThemeColor>)> {
    frame
        .iter()
        .enumerate()
        .filter(|(row, colors)| {
            full_refresh || previous.get(*row).is_none_or(|old| old != *colors)
        })
        .map(|(row, colors)| (row as u8, colors.clone()))
        .collect()
}

fn sleep_until_next_frame(start: Instant) {
    let tick = Duration::from_secs_f64(1.0 / FPS);
    let elapsed = start.elapsed();
    if elapsed < tick {
        thread::sleep(tick - elapsed);
    }
}

fn schedule_bloom_recovery(device_handle: DeviceHandle, columns: usize, current_generation: u32) {
    match thread::Builder::new()
        .name("blade-audio-bloom-recovery".to_string())
        .spawn(move || {
            thread::sleep(BLOOM_RECOVERY_DELAY);
            if THREAD_GENERATION.load(Ordering::SeqCst) == current_generation {
                AudioBloomEffect::start(device_handle, columns);
            }
        }) {
        Ok(_handle) => {}
        Err(error) => {
            warn!(%error, "Failed to schedule audio bloom recovery");
        }
    }
}
