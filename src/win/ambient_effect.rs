use crate::razer::device_handle::DeviceHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

static AMBIENT_EFFECT: AtomicBool = AtomicBool::new(false);
const FPS: u64 = 10;
const CYCLE_SPEED: Duration = Duration::from_secs(3);
const LERP_BRIGHT_FACTOR: f32 = 0.3;
const LERP_DARK_FACTOR: f32 = 0.1;
const DARK_MODE_SENSITIVITY: f32 = 0.10;

pub struct AmbientEffect {}

impl AmbientEffect {
    pub fn start(device_handle: DeviceHandle) {
        if AMBIENT_EFFECT.load(Ordering::SeqCst) {
            return;
        }
        AMBIENT_EFFECT.store(true, Ordering::SeqCst);

        thread::spawn(move || {
            unsafe {
                let h_dc_screen = GetDC(None);
                let h_dc_mem = CreateCompatibleDC(h_dc_screen);

                let s_w = GetSystemMetrics(SM_CXSCREEN);
                let s_h = GetSystemMetrics(SM_CYSCREEN);

                // We capture into a buffer that can hold our "Grid Net"
                // 8 horizontal rows (s_w wide) + 8 vertical columns (s_h high)
                let total_sample_pixels = (s_w * 8) + (s_h * 8);
                let h_bitmap = CreateCompatibleBitmap(h_dc_screen, s_w.max(s_h), 16);
                SelectObject(h_dc_mem, h_bitmap);

                let mut bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: s_w.max(s_h),
                        biHeight: -16,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0 as u32,
                        ..Default::default()
                    },
                    ..Default::default()
                };

                let mut engine = ColorEngine::new();
                let mut scan_data = vec![0u8; (s_w.max(s_h) * 16 * 4) as usize];

                while AMBIENT_EFFECT.load(Ordering::SeqCst) {
                    let start = Instant::now();

                    // 1. Capture 8 Horizontal Scanlines (distributed across height)
                    for i in 0..8 {
                        let y = (s_h / 9) * (i + 1);
                        BitBlt(h_dc_mem, 0, i, s_w, 1, h_dc_screen, 0, y, SRCCOPY);
                    }

                    // 2. Capture 8 Vertical Scanlines (distributed across width)
                    for i in 0..8 {
                        let x = (s_w / 9) * (i + 1);
                        BitBlt(h_dc_mem, 0, i + 8, 1, s_h, h_dc_screen, x, 0, SRCCOPY);
                    }

                    // 3. Move the "Net" into RAM
                    GetDIBits(
                        h_dc_mem,
                        h_bitmap,
                        0,
                        16,
                        Some(scan_data.as_mut_ptr() as *mut _),
                        &mut bmi,
                        DIB_RGB_COLORS,
                    );

                    // 4. Process the data
                    let (r, g, b) = engine.tick(&scan_data, s_w, s_h);
                    device_handle.keyboard_color(r, g, b);
                    print_color_preview(r, g, b);

                    // 15 FPS Target
                    let elapsed = start.elapsed();
                    let delay = Duration::from_millis(1000 / FPS);
                    if elapsed < delay {
                        println!("under threshold by: {:?}", delay - elapsed);
                        thread::sleep(delay - elapsed);
                    }
                }

                DeleteObject(h_bitmap);
                DeleteDC(h_dc_mem);
                ReleaseDC(None, h_dc_screen);
            }
        });
    }

    pub fn stop() {
        AMBIENT_EFFECT.store(false, Ordering::SeqCst);
    }
}

struct ColorEngine {
    smooth_rgb: (f32, f32, f32),
    cycle_start: Instant,
    color_index: usize,
}

impl ColorEngine {
    fn new() -> Self {
        Self {
            smooth_rgb: (0.0, 0.0, 0.0),
            cycle_start: Instant::now(),
            color_index: 0,
        }
    }
    fn tick(&mut self, data: &[u8], s_w: i32, s_h: i32) -> (u8, u8, u8) {
        let mut vibrant_count = 0;
        let mut candidates = [((0u8, 0u8, 0u8), 0.0f32); 4];
        let mut c_idx = 0;

        let stride = (s_w.max(s_h) * 4) as usize;
        let mut total_samples = 0;

        for row in 0..16 {
            let row_offset = row * stride;
            let limit = if row < 8 {
                s_w as usize * 4
            } else {
                s_h as usize * 4
            };

            for i in (0..limit).step_by(16) {
                total_samples += 1;
                let b = data[row_offset + i];
                let g = data[row_offset + i + 1];
                let r = data[row_offset + i + 2];

                let (_h, s, v) = rgb_to_hsv(r, g, b);

                // 1. INCREASE THE FLOOR: Ignore anything under 15% saturation.
                // This immediately kills the 38,38,38 VS Code grey.
                if v < 0.15 || s < 0.15 {
                    continue;
                }

                vibrant_count += 1;

                // Formula to prioritize neon colours
                // Saturation^6 makes "pale" colors (like maroon) lose their voting rights.
                // Value^4 ensures "dark" vibrant colors are ignored in favor of "bright" ones.
                let weight = s.powi(6) * v.powi(4);

                let bucket = (r & 0xF8, g & 0xF8, b & 0xF8);

                let mut found = false;
                for j in 0..c_idx {
                    if candidates[j].0 == bucket {
                        candidates[j].1 += weight;
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

        if vibrant_count > 0 {
            let threshold = (total_samples as f32 * DARK_MODE_SENSITIVITY) as usize;

            // 1. SORT THE CANDIDATES
            for i in 0..c_idx {
                for j in (i + 1)..c_idx {
                    if candidates[j].1 > candidates[i].1 {
                        candidates.swap(i, j);
                    }
                }
            }

            let mut target_rgb = (0u8, 0u8, 0u8);

            // 2. CHECK FOR DARK MODE VS HYBRID MODE
            if vibrant_count < threshold {
                // Cycle logic for dark mode (VS Code, etc.)
                if self.cycle_start.elapsed() >= CYCLE_SPEED {
                    self.color_index = (self.color_index + 1) % c_idx.max(1);
                    self.cycle_start = Instant::now();
                }
                target_rgb = candidates[self.color_index.min(3)].0;
            } else {
                // NORMAL MODE: Find the "Top 2 Average"
                self.color_index = 0; // Reset index for when we go back to dark mode

                if c_idx >= 2 {
                    let c1 = candidates[0].0;
                    let c2 = candidates[1].0;
                    let w1 = candidates[0].1;
                    let w2 = candidates[1].1;
                    let total_w = w1 + w2;

                    // Weighted average: favors the #1 color slightly
                    target_rgb = (
                        ((c1.0 as f32 * w1 + c2.0 as f32 * w2) / total_w) as u8,
                        ((c1.1 as f32 * w1 + c2.1 as f32 * w2) / total_w) as u8,
                        ((c1.2 as f32 * w1 + c2.2 as f32 * w2) / total_w) as u8,
                    );
                } else {
                    target_rgb = candidates[0].0;
                }
            }

            let lerp_factor = if vibrant_count > threshold {
                LERP_BRIGHT_FACTOR
            } else {
                LERP_DARK_FACTOR
            };

            // 3. APPLY VIBRANCE AND LERP
            let (tr, tg, tb) = apply_vibrance(target_rgb.0, target_rgb.1, target_rgb.2);
            self.smooth_rgb = lerp_color(self.smooth_rgb, (tr, tg, tb), lerp_factor);
        } else {
            self.smooth_rgb = lerp_color(self.smooth_rgb, (0, 0, 0), LERP_DARK_FACTOR);
        }

        (
            self.smooth_rgb.0 as u8,
            self.smooth_rgb.1 as u8,
            self.smooth_rgb.2 as u8,
        )
    }
}

#[inline(always)]
fn lerp_color(cur: (f32, f32, f32), tar: (u8, u8, u8), fac: f32) -> (f32, f32, f32) {
    let tar_r = tar.0 as f32;
    let tar_g = tar.1 as f32;
    let tar_b = tar.2 as f32;

    // We interpolate in squared space to preserve perceived brightness (Gamma Correction)
    // This prevents the "Grey Ghost" effect during transitions.
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
    let max = if r > g {
        if r > b { r } else { b }
    } else {
        if g > b { g } else { b }
    };
    let min = if r < g {
        if r < b { r } else { b }
    } else {
        if g < b { g } else { b }
    };
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
    let (nr, ng, nb) = saturate_rgb(r, g, b, 1.1);
    normalize_to_midpoint(nr, ng, nb)
}

fn saturate_rgb(r: u8, g: u8, b: u8, intensity: f32) -> (u8, u8, u8) {
    let mut r_f = r as f32 / 255.0;
    let mut g_f = g as f32 / 255.0;
    let mut b_f = b as f32 / 255.0;

    // 1. Find the "greyscale" value (Luminance)
    let luminance = 0.2126 * r_f + 0.7152 * g_f + 0.0722 * b_f;

    // 2. Linear Interpolation toward the channels
    // If intensity > 1.0, we push colors away from the grey luminance
    r_f = luminance + (r_f - luminance) * intensity * 1.5; // Boost reds to compensate for keyboard rgb
    g_f = luminance + (g_f - luminance) * intensity * 0.8;
    b_f = luminance + (b_f - luminance) * intensity * 0.8;

    // 3. Clamp and return
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

    // 1. Calculate Perceived Luminance (The 'True' Brightness)
    let lum = (0.2126 * r_f) + (0.7152 * g_f) + (0.0722 * b_f);

    // 2. Define our target pivot (128/255 = 0.50)
    let target = 0.50;

    // 3. Calculate scaling factor
    // If lum is 0.1 (dark), factor is 5.0. If lum is 0.5, factor is 1.0.
    // we use max(0.01) to prevent division by zero on pure black.
    let mut scaling_factor = if lum < target {
        target / lum.max(0.01)
    } else {
        1.0
    };

    // 4. Apply scaling with a "Soft Ceiling"
    // This prevents colors from blowing out into pure white (255,255,255)
    // by keeping the ratios intact.
    let mut nr = r_f * scaling_factor;
    let mut ng = g_f * scaling_factor;
    let mut nb = b_f * scaling_factor;

    // Final check: If any channel exceeds 1.0, re-normalize to keep the hue
    let max_channel = nr.max(ng).max(nb);
    if max_channel > 1.0 {
        nr /= max_channel;
        ng /= max_channel;
        nb /= max_channel;
    }

    ((nr * 255.0) as u8, (ng * 255.0) as u8, (nb * 255.0) as u8)
}

fn print_color_preview(r: u8, g: u8, b: u8) {
    println!(
        "\x1b[48;2;{};{};{}m    \x1b[0m RGB: {}, {}, {}",
        r, g, b, r, g, b
    );
}
