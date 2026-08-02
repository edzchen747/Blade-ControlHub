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

