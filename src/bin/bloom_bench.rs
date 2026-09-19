//! Listens to whatever is playing and reports what the equalizer makes of it.
//!
//! Runs the production `NoteBank` and `EqualizerMatrix` over live WASAPI
//! loopback audio, with no device I/O and no keyboard output, and prints the
//! distributions behind the visuals: how often the bass band actually fills, how
//! often the row 6 bar sits near the top, whether kick detection is firing on the
//! beat, and how bass-dominant the low end really is.
//!
//! Synthetic sine tests cannot stand in for this. A real mix has harmonics,
//! percussion and several instruments at once, which is exactly what the
//! relative tests in the detector depend on.
//!
//! Usage:
//!   cargo run --release --bin bloom_bench -- [--seconds N] [--warmup N]
//!
//!   --seconds N   measurement window, default 20
//!   --warmup N    discarded settling window, default 3, so the AGC has adapted
//!                 before anything is counted
//!   --dump PATH   also write per-tick raw band magnitudes to PATH as CSV, so
//!                 the kicks can be located from the signal itself rather than
//!                 from a detector that may be mistuned
//!
//! Start the track first, then run it.

use blade_controlhub::win::audio::bloom::probe_live_audio;

fn main() {
    let mut seconds = 20u64;
    let mut warmup = 3u64;
    let mut dump: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--dump" {
            dump = Some(
                args.next()
                    .unwrap_or_else(|| fail("--dump needs a path")),
            );
            continue;
        }
        let target = match flag.as_str() {
            "--seconds" => &mut seconds,
            "--warmup" => &mut warmup,
            other => fail(&format!("unrecognised argument: {other}")),
        };
        *target = args
            .next()
            .unwrap_or_else(|| fail(&format!("{flag} needs a value")))
            .parse()
            .unwrap_or_else(|_| fail(&format!("{flag} needs a whole number")));
    }

    if seconds == 0 {
        fail("--seconds must be greater than zero");
    }

    if let Err(error) = probe_live_audio(seconds, warmup, dump.as_deref()) {
        eprintln!("bloom_bench: {error}");
        std::process::exit(1);
    }
}

fn fail(message: &str) -> ! {
    eprintln!("bloom_bench: {message}");
    std::process::exit(2);
}
