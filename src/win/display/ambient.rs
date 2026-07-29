use crate::razer::device_handle::DeviceHandle;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
#[cfg(debug_assertions)]
use tracing::trace;
use tracing::warn;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const FPS: u64 = 15;
const CYCLE_SPEED: Duration = Duration::from_secs(3);
const LERP_BRIGHT_FACTOR: f32 = 0.35;
const LERP_DARK_FACTOR: f32 = 0.15;
const DARK_MODE_SENSITIVITY: f32 = 0.10;

const HORIZONTAL_SCAN_LINES: i32 = 8;
const VERTICAL_SCAN_LINES: i32 = 13;
const TOTAL_SCAN_LINES: i32 = HORIZONTAL_SCAN_LINES + VERTICAL_SCAN_LINES;

pub static THREAD_GENERATION: AtomicU32 = AtomicU32::new(0);
static AMBIENT_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

pub struct AmbientEffect {}

impl AmbientEffect {
    pub fn start(device_handle: DeviceHandle) {
        Self::stop();
        let current_generation = THREAD_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

        match thread::Builder::new()
            .name("blade-ambient-effect".to_string())
            .spawn(move || run_ambient_loop(device_handle, current_generation))
        {
            Ok(handle) => {
                *ambient_thread() = Some(handle);
            }
            Err(error) => {
                warn!(%error, "Failed to start ambient effect thread");
            }
        }
    }

    pub fn stop() {
        THREAD_GENERATION.fetch_add(1, Ordering::SeqCst);
        join_ambient_thread();
    }
}

fn ambient_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    AMBIENT_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_ambient_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = ambient_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current ambient effect thread during shutdown");
        return;
    }

    if handle.join().is_err() {
        warn!("Ambient effect thread panicked during shutdown");
    }
}

fn run_ambient_loop(device_handle: DeviceHandle, current_generation: u32) {
    let Some(mut ctx) = ScreenCaptureContext::init() else {
        warn!("Ambient effect disabled because screen capture could not be initialized");
        return;
    };

    let mut engine = ColorEngine::new();
    let mut scan_data = vec![0u8; ctx.scan_buffer_len()];
    let mut frame_count: u32 = 0;

    while THREAD_GENERATION.load(Ordering::SeqCst) == current_generation {
        let start = Instant::now();
        frame_count = frame_count.wrapping_add(1);

        if ctx.capture_scanlines(frame_count, &mut scan_data) {
            let (r, g, b) = engine.tick(&scan_data, ctx.s_w, ctx.s_h);
            print_color_preview(r, g, b);
            device_handle.set_keyboard_color(r, g, b);
        } else {
            warn!("Ambient effect stopped because screen capture failed");
            break;
        }

        sleep_until_next_frame(start);
    }
}

// --- Screen Capture ---

struct ScreenCaptureContext {
    _selected_bitmap: ObjectSelection,
    bitmap: OwnedBitmap,
    mem_dc: CompatibleDc,
    screen_dc: DesktopDc,
    bmi: BITMAPINFO,
    s_w: i32,
    s_h: i32,
    max_dim: i32,
}

impl ScreenCaptureContext {
    fn init() -> Option<Self> {
        let screen_dc = DesktopDc::acquire()?;
        let mem_dc = CompatibleDc::create(screen_dc.hdc)?;

        let s_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let s_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let max_dim = s_w.max(s_h);
        if s_w <= 0 || s_h <= 0 || max_dim <= 0 {
            return None;
        }

        let bitmap = OwnedBitmap::create_compatible(screen_dc.hdc, max_dim, TOTAL_SCAN_LINES)?;
        let selected_bitmap = ObjectSelection::select(mem_dc.hdc, bitmap.handle)?;

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: max_dim,
                biHeight: -TOTAL_SCAN_LINES, // Negative for top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        Some(Self {
            _selected_bitmap: selected_bitmap,
            bitmap,
            mem_dc,
            screen_dc,
            bmi,
            s_w,
            s_h,
            max_dim,
        })
    }

    fn scan_buffer_len(&self) -> usize {
        (self.max_dim * TOTAL_SCAN_LINES * 4) as usize
    }

    fn capture_scanlines(&mut self, frame_count: u32, scan_data: &mut [u8]) -> bool {
        if scan_data.len() < self.scan_buffer_len() {
            return false;
        }

        let mut rng_seed = frame_count;

        let captured_lines = self.capture_horizontal_lines(&mut rng_seed)
            + self.capture_vertical_lines(&mut rng_seed);
        if captured_lines == 0 {
            return false;
        }

        let copied_lines = unsafe {
            GetDIBits(
                self.mem_dc.hdc,
                self.bitmap.handle,
                0,
                TOTAL_SCAN_LINES as u32,
                Some(scan_data.as_mut_ptr() as *mut _),
                &mut self.bmi,
                DIB_RGB_COLORS,
            )
        };
        copied_lines == TOTAL_SCAN_LINES
    }

    fn capture_horizontal_lines(&self, rng_seed: &mut u32) -> usize {
        let mut captured = 0;
        for i in 0..HORIZONTAL_SCAN_LINES {
            *rng_seed = rng_seed.wrapping_mul(1103515245).wrapping_add(12345);
            let jitter = (*rng_seed % 21) as i32 - 10;
            let y = ((self.s_h / (HORIZONTAL_SCAN_LINES + 1)) * (i + 1) + jitter)
                .clamp(0, self.s_h - 1);
            if unsafe {
                BitBlt(
                    self.mem_dc.hdc,
                    0,
                    i,
                    self.s_w,
                    4,
                    self.screen_dc.hdc,
                    0,
                    y,
                    SRCCOPY,
                )
            }
            .is_ok()
            {
                captured += 1;
            }
        }
        captured
    }

    fn capture_vertical_lines(&self, rng_seed: &mut u32) -> usize {
        let mut captured = 0;
        for i in 0..VERTICAL_SCAN_LINES {
            *rng_seed = rng_seed.wrapping_mul(1103515245).wrapping_add(12345);
            let jitter = (*rng_seed % 21) as i32 - 10;
            let x =
                ((self.s_w / (VERTICAL_SCAN_LINES + 1)) * (i + 1) + jitter).clamp(0, self.s_w - 1);
            if unsafe {
                BitBlt(
                    self.mem_dc.hdc,
                    0,
                    i + HORIZONTAL_SCAN_LINES,
                    4,
                    self.s_h,
                    self.screen_dc.hdc,
                    x,
                    0,
                    SRCCOPY,
                )
            }
            .is_ok()
            {
                captured += 1;
            }
        }
        captured
    }
}

struct DesktopDc {
    hdc: HDC,
}

impl DesktopDc {
    fn acquire() -> Option<Self> {
        let hdc = unsafe { GetDC(None) };
        (!hdc.0.is_null()).then_some(Self { hdc })
    }
}

impl Drop for DesktopDc {
    fn drop(&mut self) {
        let _ = unsafe { ReleaseDC(None, self.hdc) };
    }
}

struct CompatibleDc {
    hdc: HDC,
}

impl CompatibleDc {
    fn create(source: HDC) -> Option<Self> {
        let hdc = unsafe { CreateCompatibleDC(source) };
        (!hdc.0.is_null()).then_some(Self { hdc })
    }
}

impl Drop for CompatibleDc {
    fn drop(&mut self) {
        let _ = unsafe { DeleteDC(self.hdc) };
    }
}

struct OwnedBitmap {
    handle: HBITMAP,
}

impl OwnedBitmap {
    fn create_compatible(source: HDC, width: i32, height: i32) -> Option<Self> {
        let handle = unsafe { CreateCompatibleBitmap(source, width, height) };
        (!handle.0.is_null()).then_some(Self { handle })
    }
}

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        let _ = unsafe { DeleteObject(self.handle) };
    }
}

struct ObjectSelection {
    hdc: HDC,
    old_object: HGDIOBJ,
}

impl ObjectSelection {
    fn select(hdc: HDC, bitmap: HBITMAP) -> Option<Self> {
        let old_object = unsafe { SelectObject(hdc, bitmap) };
        (!old_object.0.is_null()).then_some(Self { hdc, old_object })
    }
}

impl Drop for ObjectSelection {
    fn drop(&mut self) {
        let _ = unsafe { SelectObject(self.hdc, self.old_object) };
    }
}

fn sleep_until_next_frame(start: Instant) {
    let delay = Duration::from_millis(1000 / FPS);
    let elapsed = start.elapsed();
    if elapsed < delay {
        thread::sleep(delay - elapsed);
    }
}

// --- Color Engine ---

struct ColorEngine {
    smooth_rgb: (f32, f32, f32),
    cycle_start: Instant,
    color_index: usize,
    prev_avg_rgb: (f32, f32, f32),
}

struct ScanAnalysis {
    vibrant_count: usize,
    sample_count: usize,
    black_screen: bool,
    top_colors: [(u8, u8, u8); 3],
}

impl ColorEngine {
    fn new() -> Self {
        Self {
            smooth_rgb: (0.0, 0.0, 0.0),
            cycle_start: Instant::now(),
            color_index: 0,
            prev_avg_rgb: (0.0, 0.0, 0.0),
        }
    }

    fn tick(&mut self, data: &[u8], s_w: i32, s_h: i32) -> (u8, u8, u8) {
        let analysis = self.analyze_scan_data(data, s_w, s_h);
        self.apply_smoothing(&analysis);

        (
            self.smooth_rgb.0 as u8,
            self.smooth_rgb.1 as u8,
            self.smooth_rgb.2 as u8,
        )
    }

    fn analyze_scan_data(&mut self, data: &[u8], s_w: i32, s_h: i32) -> ScanAnalysis {
        if s_w <= 0 || s_h <= 0 {
            self.prev_avg_rgb = (0.0, 0.0, 0.0);
            return empty_scan_analysis();
        }

        let stride = (s_w.max(s_h) * 4) as usize;
        let mut vibrant_count = 0;
        let mut candidates = [((0u8, 0u8, 0u8), 0.0f32); 4];
        let mut c_idx = 0;
        let mut total_r = 0.0;
        let mut total_g = 0.0;
        let mut total_b = 0.0;
        let mut sample_count = 0;
        let mut black_screen = true;

        for row in 0..TOTAL_SCAN_LINES as usize {
            let row_offset = row * stride;
            let limit = if row < HORIZONTAL_SCAN_LINES as usize {
                s_w as usize * 4
            } else {
                s_h as usize * 4
            };

            for i in (0..limit).step_by(16) {
                if row_offset + i + 2 >= data.len() {
                    break;
                }

                let b = data[row_offset + i];
                let g = data[row_offset + i + 1];
                let r = data[row_offset + i + 2];

                total_r += r as f32;
                total_g += g as f32;
                total_b += b as f32;
                sample_count += 1;

                let (_, s, v) = rgb_to_hsv(r, g, b);
                if v > 0.15 {
                    black_screen = false;
                }
                if v < 0.15 || s < 0.15 {
                    continue;
                }

                let perceived_luminance =
                    (0.2126 * total_r) + (0.7152 * total_g) + (0.0722 * total_b);

                vibrant_count += 1;
                let weight = s.powi(2) * perceived_luminance;
                let bucket = (r & 0xF0, g & 0xF0, b & 0xF0);

                let mut found = false;
                for c in candidates.iter_mut().take(c_idx) {
                    if c.0 == bucket {
                        c.1 += weight;
                        found = true;
                        break;
                    }
                }
                if !found && c_idx < 4 {
                    candidates[c_idx] = (bucket, weight);
                    c_idx += 1;
                }
            }
        }

        if sample_count == 0 {
            self.prev_avg_rgb = (0.0, 0.0, 0.0);
            return empty_scan_analysis();
        }

        self.prev_avg_rgb = (
            total_r / sample_count as f32,
            total_g / sample_count as f32,
            total_b / sample_count as f32,
        );

        candidates[..c_idx].sort_by(|a, b| b.1.total_cmp(&a.1));

        let mut top_colors = [(0u8, 0u8, 0u8); 3];
        for (i, candidate) in candidates.iter().take(3).enumerate() {
            top_colors[i] = candidate.0;
        }

        ScanAnalysis {
            vibrant_count,
            sample_count,
            black_screen,
            top_colors,
        }
    }

    fn apply_smoothing(&mut self, analysis: &ScanAnalysis) {
        if analysis.vibrant_count > 0 {
            let threshold = (analysis.sample_count as f32 * DARK_MODE_SENSITIVITY) as usize;
            let target_rgb = self.select_target_color(analysis, threshold);

            let (tr, tg, tb) = apply_vibrance(target_rgb.0, target_rgb.1, target_rgb.2);
            let factor = if analysis.vibrant_count > threshold || analysis.black_screen {
                LERP_BRIGHT_FACTOR
            } else {
                LERP_DARK_FACTOR
            };
            self.smooth_rgb = lerp_color(self.smooth_rgb, (tr, tg, tb), factor);
        } else {
            // If no colours, set keyboard light to black (2) or grey (130)
            let v = 2 + !analysis.black_screen as u8 * 128;
            self.smooth_rgb = lerp_color(self.smooth_rgb, (v, v, v), LERP_BRIGHT_FACTOR);
        }
    }

    fn select_target_color(&mut self, analysis: &ScanAnalysis, threshold: usize) -> (u8, u8, u8) {
        if analysis.vibrant_count < threshold {
            if self.cycle_start.elapsed() >= CYCLE_SPEED {
                self.color_index = (self.color_index + 1) % 3;
                self.cycle_start = Instant::now();
            }
            analysis
                .top_colors
                .get(self.color_index)
                .copied()
                .unwrap_or_default()
        } else {
            self.color_index = 0;
            analysis.top_colors.first().copied().unwrap_or_default()
        }
    }
}

fn empty_scan_analysis() -> ScanAnalysis {
    ScanAnalysis {
        vibrant_count: 0,
        sample_count: 0,
        black_screen: true,
        top_colors: [(0, 0, 0); 3],
    }
}

// --- Color Utilities ---

#[inline(always)]
fn lerp_color(cur: (f32, f32, f32), tar: (u8, u8, u8), fac: f32) -> (f32, f32, f32) {
    let tar_r = tar.0 as f32;
    let tar_g = tar.1 as f32;
    let tar_b = tar.2 as f32;

    (
        ((1.0 - fac) * cur.0.powi(2) + fac * tar_r.powi(2)).sqrt(),
        ((1.0 - fac) * cur.1.powi(2) + fac * tar_g.powi(2)).sqrt(),
        ((1.0 - fac) * cur.2.powi(2) + fac * tar_b.powi(2)).sqrt(),
    )
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let s = if max == 0.0 { 0.0 } else { d / max };
    let mut h = 0.0;
    if d != 0.0 {
        if max == r {
            h = (g - b) / d + (if g < b { 6.0 } else { 0.0 });
        } else if max == g {
            h = (b - r) / d + 2.0;
        } else {
            h = (r - g) / d + 4.0;
        }
        h /= 6.0;
    }
    (h, s, max)
}

#[inline(always)]
fn apply_vibrance(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (nr, ng, nb) = saturate_rgb(r, g, b, 2.0);
    normalize_to_midpoint(nr, ng, nb)
}

fn saturate_rgb(r: u8, g: u8, b: u8, intensity: f32) -> (u8, u8, u8) {
    let mut r_f = r as f32 * 1.2 / 255.0;
    let mut g_f = g as f32 * 1.1 / 255.0;
    let mut b_f = b as f32 * 0.8 / 255.0;

    let luminance = 0.2126 * r_f + 0.7152 * g_f + 0.0722 * b_f;

    r_f = luminance + (r_f - luminance) * intensity;
    g_f = luminance + (g_f - luminance) * intensity;
    b_f = luminance + (b_f - luminance) * intensity;

    let max_val = r_f.max(g_f).max(b_f);
    let dominant_gamma = 0.6;

    if max_val > 0.0 {
        if r_f == max_val {
            r_f = r_f.powf(dominant_gamma);
        } else if g_f == max_val {
            g_f = g_f.powf(dominant_gamma);
        } else {
            b_f = b_f.powf(dominant_gamma);
        }
    }

    (
        (r_f.clamp(0.0, 1.0) * 255.0) as u8,
        (g_f.clamp(0.0, 1.0) * 255.0) as u8,
        (b_f.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

fn normalize_to_midpoint(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let r_f = r as f32 / 255.0;
    let g_f = g as f32 / 255.0;
    let b_f = b as f32 / 255.0;
    let lum = (0.2126 * r_f) + (0.7152 * g_f) + (0.0722 * b_f);
    let target = 0.3;

    let scaling_factor = if lum < target {
        target / lum.max(0.01)
    } else {
        1.0
    };

    let mut nr = r_f * scaling_factor;
    let mut ng = g_f * scaling_factor;
    let mut nb = b_f * scaling_factor;

    let max_channel = nr.max(ng).max(nb);
    if max_channel > 1.0 {
        nr /= max_channel;
        ng /= max_channel;
        nb /= max_channel;
    }

    ((nr * 255.0) as u8, (ng * 255.0) as u8, (nb * 255.0) as u8)
}

#[cfg(debug_assertions)]
fn print_color_preview(r: u8, g: u8, b: u8) {
    trace!(r = r, g = g, b = b, "Ambient color sample");
    // println!(
    //     "\x1b[48;2;{};{};{}m    \x1b[0m RGB: {}, {}, {}",
    //     r, g, b, r, g, b
    // );
}

#[cfg(not(debug_assertions))]
fn print_color_preview(_r: u8, _g: u8, _b: u8) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn rgb_to_hsv_identifies_red_hue() {
        let (h, s, v) = rgb_to_hsv(255, 0, 0);

        assert!(h <= 0.01 || h >= 0.99);
        assert!((s - 1.0).abs() < f32::EPSILON);
        assert!((v - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn color_engine_handles_black_scan_data_without_panicking() {
        let s_w = 8;
        let s_h = 8;
        let stride = s_w.max(s_h) as usize * 4;
        let data = vec![0u8; stride * TOTAL_SCAN_LINES as usize];
        let mut engine = ColorEngine::new();

        let (r, g, b) = engine.tick(&data, s_w, s_h);

        assert!(r <= 1);
        assert!(g <= 1);
        assert!(b <= 1);
    }

    #[test]
    fn color_engine_handles_empty_scan_data_without_panicking() {
        let mut engine = ColorEngine::new();

        let (r, g, b) = engine.tick(&[], 8, 8);

        assert!(r <= 1);
        assert!(g <= 1);
        assert!(b <= 1);
    }

    #[test]
    fn color_engine_handles_invalid_dimensions_without_panicking() {
        let mut engine = ColorEngine::new();

        let (r, g, b) = engine.tick(&[], 0, -1);

        assert!(r <= 1);
        assert!(g <= 1);
        assert!(b <= 1);
    }

    #[test]
    fn apply_vibrance_returns_visible_midpoint_color() {
        let (r, g, b) = apply_vibrance(128, 32, 16);

        assert!(r > g);
        assert!(g >= b);
        assert!(r > 70);
    }

    #[test]
    fn join_ambient_thread_drains_handle() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *ambient_thread() = Some(thread::spawn(|| {}));

        join_ambient_thread();

        assert!(ambient_thread().is_none());
    }
}
