//! Benchmarks the ambient keyboard effect's detection pipeline in isolation.
//!
//! Runs the real `AmbientDetector` — DXGI capture, colour reduction, smoothing
//! and the LED transform — on a thread configured exactly like the production
//! effect thread, with no device I/O and no other subsystem running. Resource
//! usage is read straight from the kernel: `GetThreadTimes` for CPU time
//! actually charged to the capture thread, and `GetProcessMemoryInfo` for the
//! working set it pulls in.
//!
//! Usage:
//!   cargo run --release --bin ambient_bench -- [--seconds N] [--warmup N]
//!                                              [--uncapped] [--normal-priority]
//!
//!   --seconds N         measurement window, default 30
//!   --warmup N          discarded settling window, default 3
//!   --uncapped          stress mode: drop the frame pacing entirely. The loop
//!                       then runs far faster than the display produces frames,
//!                       so most detections take the no-change fast path. Use it
//!                       to measure that path and the ceiling on capture rate,
//!                       not to model production load.
//!   --normal-priority   do not lower the thread, to see the unthrottled cost
//!   --stage NAME        how much of the capture pipeline to stand up, one of
//!                       baseline, device, idle-duplication, acquire-release,
//!                       full (default). Use this to find which stage disturbs
//!                       the compositor.
//!   --sweep             run every stage back to back and print a comparison
//!
//! While it runs, keep triggering the animations that stutter (minimise and
//! restore a window, Win+Tab). Windows 11 leaves the DWM composition counters
//! at zero, so the stutter has to be judged by watching; the stage sweep exists
//! to make that judgement a clean bisection rather than a guess.

use std::thread;
use std::time::{Duration, Instant};

use blade_controlhub::win::display::ambient::{
    AmbientDetector, CaptureStage, StageProbe, describe_adapters,
};
use windows::Win32::Foundation::{FILETIME, HWND};
use windows::Win32::Graphics::Dwm::{DWM_TIMING_INFO, DwmGetCompositionTimingInfo};
use windows::Win32::System::Com::CoIncrementMTAUsage;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentThread, GetThreadTimes};

struct Options {
    seconds: u64,
    warmup: u64,
    uncapped: bool,
    production_priority: bool,
    stage: CaptureStage,
    sweep: bool,
    adapter: Option<u32>,
    list_adapters: bool,
}

impl Options {
    fn from_args() -> Result<Self, String> {
        let mut options = Self {
            seconds: 30,
            warmup: 3,
            uncapped: false,
            production_priority: true,
            stage: CaptureStage::Full,
            sweep: false,
            adapter: None,
            list_adapters: false,
        };

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--seconds" => options.seconds = parse_value(&mut args, "--seconds")?,
                "--warmup" => options.warmup = parse_value(&mut args, "--warmup")?,
                "--uncapped" => options.uncapped = true,
                "--normal-priority" => options.production_priority = false,
                "--stage" => {
                    let name = args
                        .next()
                        .ok_or_else(|| "--stage needs a value".to_string())?;
                    options.stage = CaptureStage::parse(&name)?;
                }
                "--sweep" => options.sweep = true,
                "--adapters" => options.list_adapters = true,
                "--adapter" => options.adapter = Some(parse_value(&mut args, "--adapter")? as u32),
                other => return Err(format!("unrecognised argument: {other}")),
            }
        }

        if options.seconds == 0 {
            return Err("--seconds must be greater than zero".to_string());
        }
        Ok(options)
    }
}

fn parse_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<u64, String> {
    args.next()
        .ok_or_else(|| format!("{flag} needs a value"))?
        .parse()
        .map_err(|_| format!("{flag} needs a whole number"))
}

/// Composition frames DWM dropped or presented late. Populated on older
/// Windows builds only; see `counters_populated`.
#[derive(Clone, Copy, Default)]
struct DwmFrames {
    composed: u64,
    dropped: u64,
    late: u64,
    refresh_rate: u32,
}

impl DwmFrames {
    fn read() -> Option<Self> {
        let mut info = DWM_TIMING_INFO {
            cbSize: std::mem::size_of::<DWM_TIMING_INFO>() as u32,
            ..Default::default()
        };
        // A null window means the desktop composition as a whole.
        unsafe { DwmGetCompositionTimingInfo(HWND::default(), &mut info) }.ok()?;
        Some(Self {
            composed: info.cFramesComplete,
            dropped: info.cFramesDropped,
            late: info.cFramesLate,
            refresh_rate: info.rateCompose.uiNumerator / info.rateCompose.uiDenominator.max(1),
        })
    }

    /// Windows 11 leaves the per-frame composition counters at zero, so they
    /// cannot be used to measure stutter even though the call succeeds.
    fn counters_populated(self) -> bool {
        self.composed != 0 || self.dropped != 0 || self.late != 0
    }

    fn delta(self, earlier: Self) -> Self {
        Self {
            composed: self.composed.saturating_sub(earlier.composed),
            dropped: self.dropped.saturating_sub(earlier.dropped),
            late: self.late.saturating_sub(earlier.late),
            refresh_rate: self.refresh_rate,
        }
    }
}

/// Everything one run measured.
struct Report {
    stage: CaptureStage,
    dwm: Option<DwmFrames>,
    detections: u64,
    new_frames: Vec<Duration>,
    reused_frames: Vec<Duration>,
    wall: Duration,
    kernel_cpu: Duration,
    user_cpu: Duration,
    grid: (u32, u32),
}

fn main() {
    let options = match Options::from_args() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("ambient_bench: {error}");
            std::process::exit(2);
        }
    };

    if options.list_adapters {
        print_adapters();
        return;
    }

    if options.sweep {
        run_sweep(&options);
        return;
    }

    match run_stage(&options, options.stage) {
        Ok((report, memory_before, memory_after)) => {
            print_report(&report, memory_before, memory_after)
        }
        Err(error) => {
            eprintln!("ambient_bench: {error}");
            std::process::exit(1);
        }
    }
}

/// Runs one stage on a thread configured like the production effect thread.
fn run_stage(options: &Options, stage: CaptureStage) -> Result<(Report, u64, u64), String> {
    let memory_before = process_working_set();

    let staged = Options {
        seconds: options.seconds,
        warmup: options.warmup,
        uncapped: options.uncapped,
        production_priority: options.production_priority,
        stage,
        sweep: false,
        adapter: options.adapter,
        list_adapters: false,
    };

    let worker = thread::Builder::new()
        .name("blade-ambient-effect".to_string())
        .spawn(move || run_benchmark(&staged))
        .map_err(|err| format!("failed to spawn benchmark thread: {err}"))?;

    let report = worker
        .join()
        .map_err(|_| "benchmark thread panicked".to_string())??;

    Ok((report, memory_before, process_working_set()))
}

/// Runs every stage back to back so the one that disturbs the compositor can be
/// identified by watching the desktop during each.
fn run_sweep(options: &Options) {
    println!();
    println!("Stage sweep — keep triggering the animations that stutter throughout.");
    println!(
        "Each stage runs for {}s after a {}s warmup.",
        options.seconds, options.warmup
    );
    println!();

    let mut rows = Vec::new();
    for stage in CaptureStage::all() {
        println!("  running {:<18} {}", stage.name(), stage.description());
        match run_stage(options, stage) {
            Ok((report, _, _)) => rows.push(report),
            Err(error) => eprintln!("  {} failed: {error}", stage.name()),
        }
    }

    let have_dwm_counters = rows
        .iter()
        .any(|row| row.dwm.is_some_and(|dwm| dwm.counters_populated()));

    println!();
    println!("Cost per stage");
    println!("─────────────────────────────────────────────────────────────────────");
    println!(
        "  {:<18} {:>12} {:>12} {:>12}",
        "stage", "cpu ms", "captures/s", "dwm dropped"
    );
    for report in &rows {
        let wall = report.wall.as_secs_f64();
        let cpu = (report.kernel_cpu + report.user_cpu).as_secs_f64() * 1000.0;
        let dropped = match report.dwm.filter(|dwm| dwm.counters_populated()) {
            Some(dwm) => dwm.dropped.to_string(),
            None => "n/a".to_string(),
        };
        println!(
            "  {:<18} {:>12.1} {:>12.1} {:>12}",
            report.stage.name(),
            cpu,
            report.new_frames.len() as f64 / wall,
            dropped
        );
    }
    println!();

    if !have_dwm_counters {
        println!("Windows did not populate the DWM composition counters, so the");
        println!("stutter cannot be read off a number here. Watch the animations");
        println!("during each stage above and note the first one that stutters:");
        println!();
        println!("  baseline          -> stutters? something other than ambient capture");
        println!("  device            -> creating the D3D11 device is enough to disturb it");
        println!("  idle-duplication  -> holding the duplication session is the cause");
        println!("  acquire-release   -> the per-frame acquire/release cadence is");
        println!("  full              -> the copy and readback are");
        println!("  wgc-sparse        -> the WindowsGraphicsCapture path is");
        println!();
        println!("Alternative frame sources:");
        println!("  wgc-idle / wgc    Windows.Graphics.Capture. Known to cause");
        println!("                    stutter in games, so desktop-smooth is not");
        println!("                    on its own enough to adopt it.");
        println!("  gdi               BitBlt scanlines, the pre-DXGI source. Opens");
        println!("                    no capture session, but cannot see overlay");
        println!("                    or exclusive-fullscreen content.");
        println!();
    }
}

fn run_benchmark(options: &Options) -> Result<Report, String> {
    // Windows.Graphics.Capture is WinRT and its activation factories are cached
    // process-wide, so pin the MTA rather than joining it per-thread. Mirrors
    // what the effect thread does. Harmless for the other stages.
    let _ = unsafe { CoIncrementMTAUsage() };

    if options.production_priority {
        AmbientDetector::apply_production_thread_priority();
    }

    let grid = AmbientDetector::sample_grid();

    let mut probe = StageProbe::new_on_adapter(options.stage, options.adapter)?;

    // Let DXGI hand over its first real frames and the smoother settle before
    // anything is counted.
    let warmup_end = Instant::now() + Duration::from_secs(options.warmup);
    while Instant::now() < warmup_end {
        let start = Instant::now();
        probe.step()?;
        if !options.uncapped {
            AmbientDetector::pace_after(start);
        }
    }

    let dwm_before = DwmFrames::read();
    let (kernel_before, user_before) = thread_cpu_time()?;
    let measure_start = Instant::now();
    let measure_end = measure_start + Duration::from_secs(options.seconds);

    let mut detections = 0u64;
    let mut new_frames = Vec::new();
    let mut reused_frames = Vec::new();

    while Instant::now() < measure_end {
        let start = Instant::now();
        let captured = probe.step()?;
        let elapsed = start.elapsed();
        detections += 1;

        if captured {
            new_frames.push(elapsed);
        } else {
            reused_frames.push(elapsed);
        }

        if !options.uncapped {
            AmbientDetector::pace_after(start);
        }
    }

    let wall = measure_start.elapsed();
    if options.stage == CaptureStage::WgcSparse {
        eprintln!(
            "frames delivered by Windows: {} over {:.1}s ({:.1}/s)",
            probe.frames_delivered(),
            wall.as_secs_f64(),
            probe.frames_delivered() as f64 / wall.as_secs_f64()
        );
    }
    let (kernel_after, user_after) = thread_cpu_time()?;
    let dwm = match (DwmFrames::read(), dwm_before) {
        (Some(after), Some(before)) => Some(after.delta(before)),
        _ => None,
    };

    Ok(Report {
        stage: options.stage,
        dwm,
        detections,
        new_frames,
        reused_frames,
        wall,
        kernel_cpu: kernel_after.saturating_sub(kernel_before),
        user_cpu: user_after.saturating_sub(user_before),
        grid,
    })
}

/// Prints the GPU/display topology, so a hybrid-graphics machine shows which
/// adapters expose the primary display and which of them can be duplicated.
fn print_adapters() {
    let reports = match describe_adapters() {
        Ok(reports) => reports,
        Err(error) => {
            eprintln!("ambient_bench: {error}");
            std::process::exit(1);
        }
    };

    println!();
    println!("DXGI adapters and outputs");
    println!("───────────────────────────────────────────────");
    for adapter in &reports {
        println!(
            "  adapter {}  {}  (vendor 0x{:04x}, {} MB dedicated)",
            adapter.index,
            adapter.description,
            adapter.vendor_id,
            adapter.dedicated_video_memory / (1024 * 1024)
        );
        if adapter.outputs.is_empty() {
            println!("      (no outputs)");
        }
        for output in &adapter.outputs {
            let marks = [
                output.attached.then_some("attached"),
                output.is_primary.then_some("PRIMARY"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
            println!("      {} [{}]", output.name, marks);
            match &output.duplicable {
                Ok(()) => println!("          DuplicateOutput: ok"),
                Err(error) => println!("          DuplicateOutput: FAILED ({error})"),
            }
        }
    }
    println!();
    println!("Run a stage against a specific adapter with --adapter N, e.g.");
    println!("  ambient_bench.exe --stage idle-duplication --adapter 1 --seconds 20");
    println!();
}

// ── Kernel-reported resource usage ──────────────────────────────────────────

/// CPU time charged to the calling thread, at the kernel's 100ns resolution.
fn thread_cpu_time() -> Result<(Duration, Duration), String> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
        .map_err(|err| format!("GetThreadTimes: {err}"))?;
    }

    Ok((filetime_to_duration(kernel), filetime_to_duration(user)))
}

fn filetime_to_duration(filetime: FILETIME) -> Duration {
    let ticks = ((filetime.dwHighDateTime as u64) << 32) | filetime.dwLowDateTime as u64;
    Duration::from_nanos(ticks * 100)
}

fn process_working_set() -> u64 {
    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

    let read = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) };
    if read.is_err() {
        return 0;
    }
    counters.WorkingSetSize as u64
}

// ── Reporting ───────────────────────────────────────────────────────────────

fn print_report(report: &Report, memory_before: u64, memory_after: u64) {
    let wall = report.wall.as_secs_f64();
    let cpu = report.kernel_cpu + report.user_cpu;
    let cpu_share = cpu.as_secs_f64() / wall * 100.0;
    let cores = thread::available_parallelism().map_or(1.0, |n| n.get() as f64);

    println!();
    println!("Ambient detection benchmark");
    println!("───────────────────────────────────────────────");
    println!(
        "  stage            {} ({})",
        report.stage.name(),
        report.stage.description()
    );
    println!("  sample grid      {}x{}", report.grid.0, report.grid.1);
    println!(
        "  target rate      {:.0} fps",
        AmbientDetector::target_fps()
    );
    println!("  window           {wall:.1}s");
    println!();

    println!("Throughput");
    println!(
        "  detections       {} ({:.1}/s)",
        report.detections,
        report.detections as f64 / wall
    );
    println!(
        "  new frames       {} ({:.1}/s, {:.0}% of detections)",
        report.new_frames.len(),
        report.new_frames.len() as f64 / wall,
        percentage(report.new_frames.len(), report.detections)
    );
    println!(
        "  reused colour    {} ({:.0}% of detections)",
        report.reused_frames.len(),
        percentage(report.reused_frames.len(), report.detections)
    );
    println!();

    println!("CPU (charged to the capture thread by the kernel)");
    println!(
        "  user             {:>9.1} ms",
        report.user_cpu.as_secs_f64() * 1000.0
    );
    println!(
        "  kernel           {:>9.1} ms",
        report.kernel_cpu.as_secs_f64() * 1000.0
    );
    println!(
        "  total            {:>9.1} ms  =  {:.2}% of one core, {:.3}% of {:.0} cores",
        cpu.as_secs_f64() * 1000.0,
        cpu_share,
        cpu_share / cores,
        cores
    );
    if report.detections > 0 {
        println!(
            "  per detection    {:>9.3} ms",
            cpu.as_secs_f64() * 1000.0 / report.detections as f64
        );
    }
    println!();

    println!("Memory (process working set)");
    println!("  before           {:>9.2} MB", bytes_to_mb(memory_before));
    println!("  after            {:>9.2} MB", bytes_to_mb(memory_after));
    println!(
        "  delta            {:>+9.2} MB",
        bytes_to_mb(memory_after) - bytes_to_mb(memory_before)
    );
    println!();

    println!("DWM composition");
    match report.dwm.filter(|dwm| dwm.counters_populated()) {
        Some(dwm) => {
            println!("  compose rate     {:>9} Hz", dwm.refresh_rate);
            println!("  frames composed  {:>9}", dwm.composed);
            println!(
                "  frames dropped   {:>9}  ({:.2}/s)",
                dwm.dropped,
                dwm.dropped as f64 / wall
            );
            println!(
                "  frames late      {:>9}  ({:.2}/s)",
                dwm.late,
                dwm.late as f64 / wall
            );
        }
        None => {
            println!("  per-frame counters unavailable on this Windows build.");
            println!("  Judge this stage by watching the animations instead.");
        }
    }
    println!();

    print_latency(
        "Detection latency, new frame (capture + reduce + smooth)",
        &report.new_frames,
    );
    print_latency(
        "Detection latency, desktop unchanged (fast path)",
        &report.reused_frames,
    );
}

fn print_latency(label: &str, samples: &[Duration]) {
    println!("{label}");
    if samples.is_empty() {
        println!("  (no samples)");
        println!();
        return;
    }

    let mut sorted: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("durations are never NaN"));

    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    println!("  n                {:>9}", sorted.len());
    println!("  mean             {mean:>9.3} ms");
    println!("  p50              {:>9.3} ms", percentile(&sorted, 0.50));
    println!("  p95              {:>9.3} ms", percentile(&sorted, 0.95));
    println!("  p99              {:>9.3} ms", percentile(&sorted, 0.99));
    println!("  max              {:>9.3} ms", sorted[sorted.len() - 1]);
    println!();
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (fraction * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn percentage(part: usize, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    part as f64 / whole as f64 * 100.0
}

fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
