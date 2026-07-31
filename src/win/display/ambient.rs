use crate::razer::device_handle::DeviceHandle;
use crate::win::display::topology::{primary_display_device_name, wide_slice_to_os_string};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
#[cfg(debug_assertions)]
use tracing::trace;
use tracing::warn;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter1,
    IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
};
use windows::core::Interface;

const FPS: f64 = 15.0;
const SAMPLE_WIDTH: u32 = 32;
const SAMPLE_HEIGHT: u32 = 18;
const LERP_FACTOR: f32 = 0.6;
const BIN_LEVELS: usize = 16;
const BIN_COUNT: usize = BIN_LEVELS * BIN_LEVELS * BIN_LEVELS;

const BLACK_MAX_CHANNEL: u8 = 16;
const BLACK_LUMA_CUTOFF: u8 = 45;
const BLACK_LUMA_MAX_CHANNEL: u8 = 80;
const BLACK_MAX_SATURATION: f32 = 0.28;
const DARK_CHROMA_VISIBILITY_FLOOR: f32 = 0.08;
const FINAL_SATURATION_BOOST: f32 = 1.5;
const RED_SATURATION_BOOST: f32 = 1.5; // Boost red colors to correct keyboard LED inaccuracy
const SMOOTHED_CHROMA_FLOOR: f32 = 0.55;
const SMOOTHED_CHROMA_PULL: f32 = 0.50;
const BLACK_FALLBACK: Rgb = Rgb { r: 2, g: 2, b: 2 };
const AMBIENT_RECOVERY_DELAY: Duration = Duration::from_secs(3);

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
    let Ok(mut capture) = DxgiSparseCapture::new() else {
        warn!("Ambient effect disabled because DXGI screen capture could not be initialized");
        schedule_ambient_recovery(device_handle, current_generation);
        return;
    };

    let mut smoother = ColorSmoother::new();

    while THREAD_GENERATION.load(Ordering::SeqCst) == current_generation {
        let start = Instant::now();

        match capture.sample() {
            Ok(color) => {
                let smoothed_rgb = smoother.smooth(color.rgb);
                print_color_preview(color, smoothed_rgb);
                let (saturated_rgb, brightness) = separate_saturated_rgb_brightness(smoothed_rgb);
                device_handle.set_keyboard_color(
                    saturated_rgb.r,
                    saturated_rgb.g,
                    saturated_rgb.b,
                    apply_gamma_u8(brightness, 0.5), //  apply gamma transform to make keyboard LEDs slightly brighter
                );
            }
            Err(error) => {
                warn!(%error, "Ambient effect stopped because DXGI screen capture failed");
                schedule_ambient_recovery(device_handle, current_generation);
                break;
            }
        }

        sleep_until_next_frame(start);
    }
}

fn schedule_ambient_recovery(device_handle: DeviceHandle, current_generation: u32) {
    match thread::Builder::new()
        .name("blade-ambient-recovery".to_string())
        .spawn(move || {
            thread::sleep(AMBIENT_RECOVERY_DELAY);
            if THREAD_GENERATION.load(Ordering::SeqCst) == current_generation {
                device_handle.display_layout_changed();
            }
        }) {
        Ok(_handle) => {}
        Err(error) => {
            warn!(%error, "Failed to schedule ambient recovery");
        }
    }
}

// --- Screen Capture ---

struct DxgiSparseCapture {
    _device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging: ID3D11Texture2D,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
    last: Option<AmbientColor>,
    reducer: AmbientReducer,
}

impl DxgiSparseCapture {
    fn new() -> Result<Self, String> {
        let primary_display = primary_display_device_name()
            .ok_or_else(|| "primary display device could not be resolved".to_string())?;
        let factory: IDXGIFactory1 =
            unsafe { CreateDXGIFactory1().map_err(|err| format!("CreateDXGIFactory1: {err}"))? };

        for adapter_index in 0.. {
            let Ok(adapter) = (unsafe { factory.EnumAdapters1(adapter_index) }) else {
                break;
            };

            for output_index in 0.. {
                let Ok(output) = (unsafe { adapter.EnumOutputs(output_index) }) else {
                    break;
                };
                let Ok(output1) = output.cast::<IDXGIOutput1>() else {
                    continue;
                };
                let Ok(desc) = (unsafe { output.GetDesc() }) else {
                    continue;
                };
                if !desc.AttachedToDesktop.as_bool() {
                    continue;
                }
                if wide_slice_to_os_string(&desc.DeviceName) != primary_display {
                    continue;
                }

                if let Ok(capture) = Self::from_output(adapter.clone(), output1) {
                    return Ok(capture);
                }
            }
        }

        Err("no duplicatable primary desktop output found".to_string())
    }

    fn from_output(adapter: IDXGIAdapter1, output: IDXGIOutput1) -> Result<Self, String> {
        let feature_levels = [D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0];
        let mut device = None;
        let mut context = None;

        unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(|err| format!("D3D11CreateDevice: {err}"))?;
        }

        let device = device.ok_or_else(|| "D3D11CreateDevice returned no device".to_string())?;
        let context = context.ok_or_else(|| "D3D11CreateDevice returned no context".to_string())?;
        let duplication = unsafe {
            output
                .DuplicateOutput(&device)
                .map_err(|err| format!("DuplicateOutput: {err}"))?
        };
        let desc = unsafe { duplication.GetDesc() };
        let width = desc.ModeDesc.Width;
        let height = desc.ModeDesc.Height;
        let format = desc.ModeDesc.Format;
        if width == 0 || height == 0 {
            return Err("duplicated output has zero dimensions".to_string());
        }

        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: SAMPLE_WIDTH,
            Height: SAMPLE_HEIGHT,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut staging = None;
        unsafe {
            device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging))
                .map_err(|err| format!("CreateTexture2D sparse staging: {err}"))?;
        }
        let staging =
            staging.ok_or_else(|| "CreateTexture2D returned no staging texture".to_string())?;

        Ok(Self {
            _device: device,
            context,
            duplication,
            staging,
            width,
            height,
            format,
            last: None,
            reducer: AmbientReducer::new(),
        })
    }

    fn sample(&mut self) -> Result<AmbientColor, String> {
        for attempt in 0..5 {
            match self.capture_frame() {
                Ok(Some(color))
                    if self.last.is_none() && attempt < 4 && is_black_fallback(color) =>
                {
                    thread::sleep(Duration::from_millis(20));
                }
                Ok(Some(color)) => {
                    self.last = Some(color);
                    return Ok(color);
                }
                Ok(None) if attempt < 4 => thread::sleep(Duration::from_millis(20)),
                Ok(None) => {
                    return self
                        .last
                        .ok_or_else(|| "DXGI had no first frame ready yet".to_string());
                }
                Err(error) => return Err(error),
            }
        }

        self.last
            .ok_or_else(|| "DXGI had no first frame ready yet".to_string())
    }

    fn capture_frame(&mut self) -> Result<Option<AmbientColor>, String> {
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        let timeout_ms = if self.last.is_some() { 0 } else { 100 };
        let acquired = unsafe {
            self.duplication
                .AcquireNextFrame(timeout_ms, &mut frame_info, &mut resource)
        };

        match acquired {
            Ok(()) => {}
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(None),
            Err(error) => return Err(format!("AcquireNextFrame: {error}")),
        }

        let result = (|| {
            let resource =
                resource.ok_or_else(|| "AcquireNextFrame returned no resource".to_string())?;
            let source = resource
                .cast::<ID3D11Texture2D>()
                .map_err(|err| format!("desktop resource cast: {err}"))?;

            self.copy_sample_grid(&source);
            let color = self.reduce_staging()?;
            Ok(color)
        })();

        let _ = unsafe { self.duplication.ReleaseFrame() };
        result.map(Some)
    }

    fn copy_sample_grid(&self, source: &ID3D11Texture2D) {
        for row in 0..SAMPLE_HEIGHT {
            let y =
                (((row * self.height) + (self.height / 2)) / SAMPLE_HEIGHT).min(self.height - 1);
            for col in 0..SAMPLE_WIDTH {
                let x =
                    (((col * self.width) + (self.width / 2)) / SAMPLE_WIDTH).min(self.width - 1);
                let source_box = D3D11_BOX {
                    left: x,
                    top: y,
                    front: 0,
                    right: x + 1,
                    bottom: y + 1,
                    back: 1,
                };
                unsafe {
                    self.context.CopySubresourceRegion(
                        &self.staging,
                        0,
                        col,
                        row,
                        0,
                        source,
                        0,
                        Some(&source_box),
                    );
                }
            }
        }
    }

    fn reduce_staging(&mut self) -> Result<AmbientColor, String> {
        self.reducer.clear();

        let staging_resource: ID3D11Resource = self
            .staging
            .cast()
            .map_err(|err| format!("staging resource cast: {err}"))?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(&staging_resource, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|err| format!("Map sparse staging texture: {err}"))?;
        }

        for row in 0..SAMPLE_HEIGHT {
            for col in 0..SAMPLE_WIDTH {
                let offset = row as usize * mapped.RowPitch as usize + col as usize * 4;
                let base = mapped.pData as *const u8;
                let c0 = unsafe { *base.add(offset) };
                let c1 = unsafe { *base.add(offset + 1) };
                let c2 = unsafe { *base.add(offset + 2) };
                let (r, g, b) = match self.format {
                    DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => (c0, c1, c2),
                    DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => (c2, c1, c0),
                    _ => (c2, c1, c0),
                };
                self.reducer.add(Rgb { r, g, b });
            }
        }

        unsafe {
            self.context.Unmap(&staging_resource, 0);
        }

        Ok(self.reducer.finish(SAMPLE_WIDTH * SAMPLE_HEIGHT))
    }
}

// --- Color Engine ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(debug_assertions), allow(dead_code))]
struct AmbientColor {
    rgb: Rgb,
    bin_rgb: Rgb,
    dominance: f32,
    ignored_black: f32,
}

struct ColorSmoother {
    smooth_rgb: (f32, f32, f32),
}

impl ColorSmoother {
    fn new() -> Self {
        Self {
            smooth_rgb: (0.0, 0.0, 0.0),
        }
    }

    fn smooth(&mut self, target: Rgb) -> Rgb {
        let smoothed_rgb = preserve_target_chroma(
            rgb_from_float_tuple(lerp_color(self.smooth_rgb, target, LERP_FACTOR)),
            target,
        );

        self.smooth_rgb = (
            smoothed_rgb.r as f32,
            smoothed_rgb.g as f32,
            smoothed_rgb.b as f32,
        );

        smoothed_rgb
    }
}

fn preserve_target_chroma(smoothed: Rgb, target: Rgb) -> Rgb {
    let target_saturation = saturation(target);
    if target_saturation < 0.35 {
        return smoothed;
    }

    let smoothed_saturation = saturation(smoothed);
    let floor = target_saturation * SMOOTHED_CHROMA_FLOOR;
    if smoothed_saturation >= floor {
        return smoothed;
    }

    let deficit = ((floor - smoothed_saturation) / floor).clamp(0.0, 1.0);
    mix_rgb(smoothed, target, SMOOTHED_CHROMA_PULL * deficit)
}

fn rgb_from_float_tuple(rgb: (f32, f32, f32)) -> Rgb {
    Rgb {
        r: rgb.0.round().clamp(0.0, 255.0) as u8,
        g: rgb.1.round().clamp(0.0, 255.0) as u8,
        b: rgb.2.round().clamp(0.0, 255.0) as u8,
    }
}

struct AmbientReducer {
    scores: [f32; BIN_COUNT],
    sum_r: [f32; BIN_COUNT],
    sum_g: [f32; BIN_COUNT],
    sum_b: [f32; BIN_COUNT],
    ignored_black: u32,
}

impl AmbientReducer {
    fn new() -> Self {
        Self {
            scores: [0.0; BIN_COUNT],
            sum_r: [0.0; BIN_COUNT],
            sum_g: [0.0; BIN_COUNT],
            sum_b: [0.0; BIN_COUNT],
            ignored_black: 0,
        }
    }

    fn clear(&mut self) {
        self.scores.fill(0.0);
        self.sum_r.fill(0.0);
        self.sum_g.fill(0.0);
        self.sum_b.fill(0.0);
        self.ignored_black = 0;
    }

    fn add(&mut self, rgb: Rgb) {
        if is_ignored_black(rgb) {
            self.ignored_black += 1;
            return;
        }

        let weight = ambient_weight(rgb);
        if weight <= 0.0 {
            return;
        }

        let bin = rgb_bin(rgb);
        self.scores[bin] += weight;
        self.sum_r[bin] += rgb.r as f32 * weight;
        self.sum_g[bin] += rgb.g as f32 * weight;
        self.sum_b[bin] += rgb.b as f32 * weight;
    }

    fn finish(&self, sampled_total: u32) -> AmbientColor {
        let considered_score: f32 = self.scores.iter().sum();
        let ignored_black = if sampled_total == 0 {
            0.0
        } else {
            self.ignored_black as f32 / sampled_total as f32
        };
        if considered_score <= f32::EPSILON {
            return AmbientColor {
                rgb: BLACK_FALLBACK,
                bin_rgb: bin_center_rgb(rgb_bin(BLACK_FALLBACK)),
                dominance: 1.0,
                ignored_black: 1.0,
            };
        }

        let (best_bin, best_score) = self
            .scores
            .iter()
            .copied()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .unwrap_or((0, 0.0));
        let top = self.bin_average(best_bin);
        let scene = self.weighted_scene_average().unwrap_or(top);
        let palette = self.salient_palette_average().unwrap_or(top);
        let rgb = boost_red_saturation(
            boost_saturation(mix_rgb(palette, scene, 0.14), FINAL_SATURATION_BOOST),
            RED_SATURATION_BOOST,
        );

        AmbientColor {
            rgb,
            bin_rgb: bin_center_rgb(rgb_bin(rgb)),
            dominance: (best_score / considered_score).clamp(0.0, 1.0),
            ignored_black,
        }
    }

    fn bin_average(&self, bin: usize) -> Rgb {
        let score = self.scores[bin].max(f32::EPSILON);
        Rgb {
            r: (self.sum_r[bin] / score).round().clamp(0.0, 255.0) as u8,
            g: (self.sum_g[bin] / score).round().clamp(0.0, 255.0) as u8,
            b: (self.sum_b[bin] / score).round().clamp(0.0, 255.0) as u8,
        }
    }

    fn salient_palette_average(&self) -> Option<Rgb> {
        let max_score = self.scores.iter().copied().fold(0.0f32, f32::max);
        if max_score <= f32::EPSILON {
            return None;
        }

        let mut total = 0.0;
        let mut r = 0.0;
        let mut g = 0.0;
        let mut b = 0.0;

        for (bin, score) in self.scores.iter().copied().enumerate() {
            if score < max_score * 0.02 {
                continue;
            }

            let weight = score.powf(0.18);
            let avg = self.bin_average(bin);
            total += weight;
            r += avg.r as f32 * weight;
            g += avg.g as f32 * weight;
            b += avg.b as f32 * weight;
        }

        if total <= f32::EPSILON {
            None
        } else {
            Some(Rgb {
                r: (r / total).round().clamp(0.0, 255.0) as u8,
                g: (g / total).round().clamp(0.0, 255.0) as u8,
                b: (b / total).round().clamp(0.0, 255.0) as u8,
            })
        }
    }

    fn weighted_scene_average(&self) -> Option<Rgb> {
        let total: f32 = self.scores.iter().sum();
        if total <= f32::EPSILON {
            return None;
        }

        Some(Rgb {
            r: (self.sum_r.iter().sum::<f32>() / total)
                .round()
                .clamp(0.0, 255.0) as u8,
            g: (self.sum_g.iter().sum::<f32>() / total)
                .round()
                .clamp(0.0, 255.0) as u8,
            b: (self.sum_b.iter().sum::<f32>() / total)
                .round()
                .clamp(0.0, 255.0) as u8,
        })
    }
}

fn sleep_until_next_frame(start: Instant) {
    let delay = Duration::from_secs_f64(1.0 / FPS);
    let elapsed = start.elapsed();
    if elapsed < delay {
        thread::sleep(delay - elapsed);
    }
}

#[inline(always)]
fn lerp_color(current: (f32, f32, f32), target: Rgb, factor: f32) -> (f32, f32, f32) {
    let target_r = target.r as f32;
    let target_g = target.g as f32;
    let target_b = target.b as f32;

    (
        lerp(current.0, target_r, factor),
        lerp(current.1, target_g, factor),
        lerp(current.2, target_b, factor),
    )
}

fn lerp(current: f32, target: f32, factor: f32) -> f32 {
    ((1.0 - factor) * current.powi(2) + factor * target.powi(2)).sqrt()
}

fn saturation(rgb: Rgb) -> f32 {
    let r = rgb.r as f32 / 255.0;
    let g = rgb.g as f32 / 255.0;
    let b = rgb.b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max <= f32::EPSILON {
        0.0
    } else {
        (max - min) / max
    }
}

fn ambient_weight(rgb: Rgb) -> f32 {
    let r = rgb.r as f32 / 255.0;
    let g = rgb.g as f32 / 255.0;
    let b = rgb.b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;
    let saturation = if max <= f32::EPSILON {
        0.0
    } else {
        chroma / max
    };
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let hue = hue_degrees(r, g, b, max, min);

    let colorfulness = saturation.powf(1.65);
    let chroma_boost = chroma.powf(0.85);
    let visibility = smoothstep(0.08, 0.55, luma)
        .max(DARK_CHROMA_VISIBILITY_FLOOR * saturation.powf(1.2) * smoothstep(0.02, 0.18, chroma));
    let low_sat_penalty = (0.12 + 0.88 * colorfulness).clamp(0.0, 1.0);
    let white_penalty =
        1.0 - 0.55 * smoothstep(0.78, 0.96, luma) * (1.0 - saturation).clamp(0.0, 1.0);
    let warm_pink_boost =
        1.0 + 0.32 * hue_window(hue, 310.0, 55.0) + 0.18 * hue_window(hue, 25.0, 45.0);
    let nature_green_boost = 1.0 + 0.16 * hue_window(hue, 135.0, 60.0);

    visibility
        * white_penalty
        * (0.10 + 2.65 * colorfulness + 0.90 * chroma_boost)
        * low_sat_penalty
        * warm_pink_boost
        * nature_green_boost
}

fn hue_degrees(r: f32, g: f32, b: f32, max: f32, min: f32) -> f32 {
    let chroma = max - min;
    if chroma <= f32::EPSILON {
        return 0.0;
    }

    let hue = if (max - r).abs() <= f32::EPSILON {
        60.0 * (((g - b) / chroma) % 6.0)
    } else if (max - g).abs() <= f32::EPSILON {
        60.0 * (((b - r) / chroma) + 2.0)
    } else {
        60.0 * (((r - g) / chroma) + 4.0)
    };

    hue.rem_euclid(360.0)
}

fn hue_window(hue: f32, center: f32, width: f32) -> f32 {
    let delta = ((hue - center + 540.0).rem_euclid(360.0) - 180.0).abs();
    (1.0 - delta / width).clamp(0.0, 1.0)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn is_ignored_black(rgb: Rgb) -> bool {
    let max_channel = rgb.r.max(rgb.g).max(rgb.b);
    max_channel <= BLACK_MAX_CHANNEL
        || (luma8(rgb) <= BLACK_LUMA_CUTOFF
            && max_channel <= BLACK_LUMA_MAX_CHANNEL
            && saturation(rgb) <= BLACK_MAX_SATURATION)
}

fn luma8(rgb: Rgb) -> u8 {
    ((54u16 * rgb.r as u16 + 183u16 * rgb.g as u16 + 19u16 * rgb.b as u16) >> 8) as u8
}

fn is_black_fallback(color: AmbientColor) -> bool {
    color.rgb == BLACK_FALLBACK && color.ignored_black >= 1.0
}

fn rgb_bin(rgb: Rgb) -> usize {
    let r = rgb.r as usize * BIN_LEVELS / 256;
    let g = rgb.g as usize * BIN_LEVELS / 256;
    let b = rgb.b as usize * BIN_LEVELS / 256;
    (r * BIN_LEVELS + g) * BIN_LEVELS + b
}

fn bin_center_rgb(bin: usize) -> Rgb {
    let step = 256 / BIN_LEVELS;
    let b = bin % BIN_LEVELS;
    let g = (bin / BIN_LEVELS) % BIN_LEVELS;
    let r = (bin / (BIN_LEVELS * BIN_LEVELS)) % BIN_LEVELS;
    Rgb {
        r: (r * step + step / 2).min(255) as u8,
        g: (g * step + step / 2).min(255) as u8,
        b: (b * step + step / 2).min(255) as u8,
    }
}

fn mix_rgb(a: Rgb, b: Rgb, b_amount: f32) -> Rgb {
    let t = b_amount.clamp(0.0, 1.0);
    Rgb {
        r: (a.r as f32 * (1.0 - t) + b.r as f32 * t)
            .round()
            .clamp(0.0, 255.0) as u8,
        g: (a.g as f32 * (1.0 - t) + b.g as f32 * t)
            .round()
            .clamp(0.0, 255.0) as u8,
        b: (a.b as f32 * (1.0 - t) + b.b as f32 * t)
            .round()
            .clamp(0.0, 255.0) as u8,
    }
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);

    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn boost_saturation(rgb: Rgb, amount: f32) -> Rgb {
    let amount = amount.max(0.0);

    let r = srgb_to_linear(rgb.r as f32 / 255.0);
    let g = srgb_to_linear(rgb.g as f32 / 255.0);
    let b = srgb_to_linear(rgb.b as f32 / 255.0);

    // Rec.709 luminance in linear RGB.
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;

    Rgb {
        r: (linear_to_srgb(luma + (r - luma) * amount) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8,
        g: (linear_to_srgb(luma + (g - luma) * amount) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8,
        b: (linear_to_srgb(luma + (b - luma) * amount) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8,
    }
}

fn boost_red_saturation(rgb: Rgb, boost_factor: f32) -> Rgb {
    let r = rgb.r as f32 / 255.0;
    let g = rgb.g as f32 / 255.0;
    let b = rgb.b as f32 / 255.0;
    let red_dominance = (r - g.max(b)).max(0.0);
    let red_mask = red_dominance * red_dominance;

    let g_new = (g * (1.0 - red_mask * boost_factor) * 255.0).round();
    let b_new = (b * (1.0 - red_mask * boost_factor) * 255.0).round();

    let r_new = ((r + red_mask * boost_factor) * 255.0).round();

    Rgb {
        r: r_new.clamp(0.0, 255.0) as u8,
        g: g_new.clamp(0.0, 255.0) as u8,
        b: b_new.clamp(0.0, 255.0) as u8,
    }
}

fn separate_saturated_rgb_brightness(rgb: Rgb) -> (Rgb, u8) {
    let brightness = rgb.r.max(rgb.g.max(rgb.b));
    if brightness == 0 {
        return (rgb, brightness);
    }

    let scale_factor = 255.0 / brightness as f32;
    let saturated_rgb = Rgb {
        r: (rgb.r as f32 * scale_factor).round().clamp(0.0, 255.0) as u8,
        g: (rgb.g as f32 * scale_factor).round().clamp(0.0, 255.0) as u8,
        b: (rgb.b as f32 * scale_factor).round().clamp(0.0, 255.0) as u8,
    };
    (saturated_rgb, brightness)
}

pub fn apply_gamma_u8(value: u8, gamma: f32) -> u8 {
    let normalized = value as f32 / 255.0;

    let gamma_corrected = normalized.powf(gamma);

    let scaled = (gamma_corrected * 255.0 + 0.5) as i32;
    scaled.clamp(0, 255) as u8
}

#[cfg(debug_assertions)]
fn print_color_preview(color: AmbientColor, smoothed: Rgb) {
    trace!(
        r = color.rgb.r,
        g = color.rgb.g,
        b = color.rgb.b,
        smoothed_r = smoothed.r,
        smoothed_g = smoothed.g,
        smoothed_b = smoothed.b,
        bin_r = color.bin_rgb.r,
        bin_g = color.bin_rgb.g,
        bin_b = color.bin_rgb.b,
        dominance = color.dominance,
        ignored_black = color.ignored_black,
        "Ambient color sample"
    );
}

#[cfg(not(debug_assertions))]
fn print_color_preview(_color: AmbientColor, _smoothed: Rgb) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn reducer_ignores_black_and_uses_visible_color() {
        let mut reducer = AmbientReducer::new();

        for _ in 0..20 {
            reducer.add(Rgb { r: 0, g: 0, b: 0 });
        }
        for _ in 0..4 {
            reducer.add(Rgb {
                r: 220,
                g: 40,
                b: 120,
            });
        }

        let color = reducer.finish(24);

        assert!(color.rgb.r > color.rgb.g);
        assert!(color.rgb.b > color.rgb.g);
        assert!(color.ignored_black > 0.80);
    }

    #[test]
    fn reducer_returns_black_fallback_when_no_visible_samples_remain() {
        let mut reducer = AmbientReducer::new();

        reducer.add(Rgb { r: 0, g: 0, b: 0 });
        reducer.add(Rgb {
            r: 12,
            g: 12,
            b: 12,
        });

        let color = reducer.finish(2);

        assert_eq!(color.rgb, BLACK_FALLBACK);
        assert!(is_black_fallback(color));
    }

    #[test]
    fn black_filter_keeps_dark_saturated_colors() {
        assert!(!is_ignored_black(Rgb { r: 24, g: 0, b: 72 }));
        assert!(is_ignored_black(Rgb {
            r: 32,
            g: 32,
            b: 32
        }));
    }

    #[test]
    fn reducer_keeps_dark_purple_dominant_over_white_ui() {
        let mut reducer = AmbientReducer::new();

        for _ in 0..360 {
            reducer.add(Rgb { r: 24, g: 0, b: 72 });
        }
        for _ in 0..80 {
            reducer.add(Rgb {
                r: 245,
                g: 245,
                b: 245,
            });
        }

        let color = reducer.finish(440);

        assert!(color.rgb.b > color.rgb.r);
        assert!(color.rgb.r > color.rgb.g);
        assert!(saturation(color.rgb) > 0.45);
    }

    #[test]
    fn ambient_weight_favors_saturated_color_over_white() {
        let saturated = ambient_weight(Rgb {
            r: 220,
            g: 40,
            b: 120,
        });
        let white = ambient_weight(Rgb {
            r: 240,
            g: 240,
            b: 240,
        });

        assert!(saturated > white);
    }

    #[test]
    fn color_smoother_eases_toward_target_without_snapping() {
        let mut smoother = ColorSmoother::new();

        let color = smoother.smooth(Rgb {
            r: 100,
            g: 100,
            b: 100,
        });

        assert!(color.r > 0);
        assert!(color.r < 100);
        assert_eq!(color.r, color.g);
        assert_eq!(color.g, color.b);
    }

    #[test]
    fn color_smoother_preserves_blue_chroma_when_leaving_grey() {
        let mut smoother = ColorSmoother {
            smooth_rgb: (180.0, 180.0, 180.0),
        };

        let color = smoother.smooth(Rgb { r: 0, g: 0, b: 255 });

        assert!(color.b > color.r + 80);
        assert!(color.b > color.g + 80);
        assert!(saturation(color) > 0.45);
    }

    #[test]
    fn color_smoother_uses_slower_factor_when_dimming() {
        let mut smoother = ColorSmoother {
            smooth_rgb: (200.0, 200.0, 200.0),
        };

        let color = smoother.smooth(Rgb {
            r: 100,
            g: 100,
            b: 100,
        });

        assert!(color.r > 100);
        assert!(color.r < 200);
        assert_eq!(color.r, color.g);
        assert_eq!(color.g, color.b);
    }

    #[test]
    fn rgb_bin_returns_center_for_same_bucket() {
        let color = Rgb {
            r: 31,
            g: 32,
            b: 33,
        };
        let center = bin_center_rgb(rgb_bin(color));

        assert_eq!(
            center,
            Rgb {
                r: 24,
                g: 40,
                b: 40
            }
        );
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
