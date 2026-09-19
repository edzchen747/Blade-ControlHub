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
