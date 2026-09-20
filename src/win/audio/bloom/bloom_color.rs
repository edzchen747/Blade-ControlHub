/// A linear 0..1 colour, the form the layers are composited in.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rgb01 {
    r: f32,
    g: f32,
    b: f32,
}

impl Rgb01 {
    const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };

    fn to_theme_color(self) -> ThemeColor {
        ThemeColor::new(
            unit_to_byte(self.r),
            unit_to_byte(self.g),
            unit_to_byte(self.b),
        )
    }
}

fn unit_to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Rec. 709 relative luminance - how bright a colour actually looks.
///
/// The weights are wildly uneven because the eye is: green carries over ten
/// times the perceived brightness of blue at the same numeric level.
fn relative_luminance(color: Rgb01) -> f32 {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b
}

/// The most saturation a hue may carry before it reads as dark.
///
/// At value 1 the luminance of a hue is linear in saturation:
///
/// ```text
/// L(s) = 1 - s * (1 - L_full)
/// ```
///
/// because desaturating raises every channel towards white. Solving that for
/// the floor gives the cap directly, with no search:
///
/// ```text
/// s = (1 - MIN_HUE_LUMINANCE) / (1 - L_full)
/// ```
///
/// A hue already above the floor yields a cap above 1 and is left alone.
fn saturation_for_hue(hue: f32) -> f32 {
    let luminance = relative_luminance(hsv_to_rgb(hue, 1.0, 1.0));
    if luminance >= MIN_HUE_LUMINANCE {
        return 1.0;
    }

    ((1.0 - MIN_HUE_LUMINANCE) / (1.0 - luminance)).clamp(0.0, 1.0)
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> Rgb01 {
    if value <= 0.0 {
        return Rgb01::BLACK;
    }

    let hue = hue.rem_euclid(1.0) * 6.0;
    let sector = hue.floor();
    let offset = hue - sector;

    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * offset);
    let t = value * (1.0 - saturation * (1.0 - offset));

    let (r, g, b) = match sector as u32 % 6 {
        0 => (value, t, p),
        1 => (q, value, p),
        2 => (p, value, t),
        3 => (p, q, value),
        4 => (t, p, value),
        _ => (value, p, q),
    };

    Rgb01 { r, g, b }
}
