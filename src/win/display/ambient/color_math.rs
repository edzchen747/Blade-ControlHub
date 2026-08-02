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


include!("color_math_tests.rs");
