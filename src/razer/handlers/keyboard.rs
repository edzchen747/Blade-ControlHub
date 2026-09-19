use librazer::device::Device;

use crate::{
    config::{ThemeColor, persist_config},
    core::shared_state::PRIMARY_MULTIMEDIA_KEYS,
    razer::{
        config::AppConfig,
        device_handle::device,
        enums::{LidLogoMode, RGBEffect},
        protocol::{HID_PACKET_ARGS_LEN, command, command_without_settings_update},
    },
    ui::{app::app, app_events::OsdEvent},
    utils::persist::PersistBuffer,
    win::audio::AudioBloomEffect,
    win::display::ambient::AmbientEffect,
};
use std::sync::atomic::Ordering;

/// The most columns a host-driven effect can address on a row.
///
/// The wire format carries 19 triplets per row, addressing columns 0..=18. The
/// real count is per-model and probed by `init_keyboard_width`, so this is only
/// the ceiling and the fallback for a device that was never probed.
pub const MATRIX_COLUMNS: usize = 19;

/// Rows in the key matrix, row 0 being the function row.
///
/// Note that rows line up with each other but columns do not: key widths and
/// stagger mean column `c` of one row is not physically above column `c` of the
/// next. Effects should not assume vertical alignment.
pub const MATRIX_ROWS: usize = 7;

pub struct KeyboardHandler<'a> {
    device: &'a Device,
    app_config: &'a mut AppConfig,
    persist_buffer: &'a PersistBuffer,
}

impl<'a> KeyboardHandler<'a> {
    pub fn new(
        device: &'a Device,
        app_config: &'a mut AppConfig,
        persist_buffer: &'a PersistBuffer,
    ) -> Self {
        Self {
            device,
            app_config,
            persist_buffer,
        }
    }


    pub fn adjust_keyboard_light(&mut self, up: bool) {
        let level_new = next_keyboard_brightness(self.app_config.read().key_lvl, up);
        self.set_keyboard_brightness(level_new);
    }

    pub fn set_keyboard_brightness(&mut self, brightness: u8) {
        app(OsdEvent::KeyboardBrightness(brightness).into());
        let _ = command(self.device, 0x0303, &[1, 5, brightness], None);
        self.app_config.get().key_lvl = brightness;
        self.persist_config();
    }

    pub fn get_keyboard_brightness(&self) -> u8 {
        command(self.device, 0x0383, &[1, 5, 0], Some(&[2]))
            .ok()
            .and_then(|result| result.first().copied())
            .unwrap_or(0)
    }

    pub fn get_primary_multimedia_keys(&self) -> bool {
        let primary_multimedia_keys = self.app_config.primary_multimedia_keys;
        PRIMARY_MULTIMEDIA_KEYS.store(primary_multimedia_keys, Ordering::SeqCst);
        primary_multimedia_keys
    }

    pub fn toggle_primary_multimedia_keys(&mut self) -> bool {
        let primary_multimedia_keys = self.app_config.primary_multimedia_keys;
        self.set_primary_multimedia_keys(!primary_multimedia_keys)
    }

    pub fn set_primary_multimedia_keys(&mut self, enabled: bool) -> bool {
        if self.app_config.primary_multimedia_keys == enabled {
            return self.get_primary_multimedia_keys();
        }

        self.app_config.primary_multimedia_keys = enabled;
        match enabled {
            true => self.enable_multimedia_keys(),
            false => self.restore_fn_keys(),
        }
        self.persist_config();
        self.get_primary_multimedia_keys()
    }


    pub fn cycle_rgb_mode(&mut self) {
        let new_rgb_effect = self.app_config.get().rgb_effect.next();
        self.set_rgb_effect(new_rgb_effect);
    }

    pub fn set_rgb_effect(&mut self, rgb_effect: RGBEffect) {
        // Ambient and Audio Bloom are driven from the host; the rest run on
        // the keyboard itself.
        let host_driven = matches!(rgb_effect, RGBEffect::Ambient | RGBEffect::AudioBloom);

        if rgb_effect == RGBEffect::Ambient {
            AmbientEffect::start(device());
        } else {
            AmbientEffect::stop();
        }

        if rgb_effect == RGBEffect::AudioBloom {
            AudioBloomEffect::start(device(), self.matrix_columns());
        } else {
            AudioBloomEffect::stop();
        }

        if host_driven {
            app(OsdEvent::RGBEffect(rgb_effect).into());
        }

        // A host-driven effect still has to put the firmware into a mode that
        // honours the per-column matrix writes, and 5 (Ambient) is that mode.
        // Audio Bloom's own discriminant is not a real firmware opcode, so it
        // rides the same carrier.
        let carrier = if rgb_effect == RGBEffect::AudioBloom {
            RGBEffect::Ambient as u8
        } else {
            rgb_effect as u8
        };
        let mut args = vec![carrier, 0];

        if rgb_effect == RGBEffect::Reactive {
            args = vec![carrier, 0, 32];
            args.extend([
                self.app_config.theme_color.r,
                self.app_config.theme_color.g,
                self.app_config.theme_color.b,
            ]);
        } else if rgb_effect == RGBEffect::Static {
            args = vec![carrier];
            args.extend([
                self.app_config.theme_color.r,
                self.app_config.theme_color.g,
                self.app_config.theme_color.b,
            ]);
        }
        let _ = command(self.device, 0x030a, &args, None);

        let _ = self.app_config.get().rgb_effect.set(&rgb_effect);
        // Firmware cannot report a host-driven state, so skip the read-back or
        // it would overwrite the OSD label with the carrier effect.
        if !host_driven {
            let effect = self.get_rgb_effect();
            app(OsdEvent::RGBEffect(effect).into());
        }
        self.persist_config();
    }

    /// How many columns this model actually lights on a row.
    ///
    /// `init_keyboard_width` probes the largest **stop** column the firmware
    /// accepts, and the stop column is inclusive, so a probed width of `W` means
    /// columns 0..=W are addressable — `W + 1` of them. Getting this off by one
    /// puts a mirrored effect's centre axis half a column out and leaves the
    /// leftmost key permanently dark.
    fn matrix_columns(&self) -> usize {
        let probed = self.app_config.keyboard_width as usize;
        if probed == 0 {
            MATRIX_COLUMNS
        } else {
            (probed + 1).min(MATRIX_COLUMNS)
        }
    }

    pub fn get_rgb_effect(&self) -> RGBEffect {
        command(self.device, 0x038a, &[0], Some(&[0]))
            .ok()
            .and_then(|result| result.first().copied())
            .unwrap_or(0)
            .into()
    }

    pub fn toggle_under_glow(&mut self) {
        let brightness = self.get_under_glow_brightness();
        self.set_under_glow_enabled(brightness == 0);
    }

    pub fn set_under_glow_enabled(&mut self, enabled: bool) {
        let new_brightness = if enabled { 255 } else { 0 };
        let _ = command(self.device, 0x0303, &[1, 38, new_brightness], None);
        let _ = command(self.device, 0x0300, &[1, 38, new_brightness / 255], None);
        app(OsdEvent::UnderGlow(new_brightness).into());
        self.app_config.get().vc_lvl = new_brightness;
        self.persist_config();
    }

    pub fn enable_under_glow(&self, brightness: u8) {
        let _ = command(self.device, 0x0300, &[1, 38, 1], None);
        let _ = command(self.device, 0x0303, &[1, 38, brightness], None);
    }

    pub fn get_under_glow_brightness(&self) -> u8 {
        let brightness = command(self.device, 0x0383, &[1, 38, 0], Some(&[2]))
            .ok()
            .and_then(|result| result.first().copied())
            .unwrap_or(0);
        let active = command(self.device, 0x0380, &[1, 38, 0], Some(&[2]))
            .ok()
            .and_then(|result| result.first().copied())
            .unwrap_or(0);
        brightness * active
    }

    pub fn set_keyboard_color(&mut self, r: u8, g: u8, b: u8, brightness: u8) {
        let width = self.app_config.keyboard_width;
        let mut args = keyboard_color_args(width, r, g, b);
        if args.len() > HID_PACKET_ARGS_LEN {
            tracing::warn!(
                len = args.len(),
                max = HID_PACKET_ARGS_LEN,
                "Skipping keyboard color command with oversized HID payload"
            );
            return;
        }
        let kb_brightness = self.app_config.get().key_lvl;
        let scaled_brightness = (kb_brightness as f32 * brightness as f32 / 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;
        let _ =
            command_without_settings_update(self.device, 0x0303, &[1, 5, scaled_brightness], None);

        for row in 0..=6 {
            if let Some(row_arg) = args.get_mut(1) {
                *row_arg = row;
            }
            let _ = command_without_settings_update(self.device, 0x030b, &args, None);
        }
    }

    /// Paints individual keys, one row per entry.
    ///
    /// Brightness is baked into the triplets rather than sent as a level, so a
    /// frame costs one HID write per row it actually changes. `reassert_brightness`
    /// re-sends the configured keyboard brightness, which the effect asks for
    /// periodically so nothing else can quietly dim it.
    pub fn set_key_rows(&mut self, rows: &[(u8, Vec<ThemeColor>)], reassert_brightness: bool) {
        if reassert_brightness {
            let kb_brightness = self.app_config.get().key_lvl;
            let _ = command_without_settings_update(
                self.device,
                0x0303,
                &[1, 5, kb_brightness],
                None,
            );
        }

        for (row, colors) in rows {
            let mut args = keyboard_row_color_args(colors);
            if args.len() > HID_PACKET_ARGS_LEN {
                tracing::warn!(
                    len = args.len(),
                    max = HID_PACKET_ARGS_LEN,
                    "Skipping key row color command with oversized HID payload"
                );
                continue;
            }
            if let Some(row_arg) = args.get_mut(1) {
                *row_arg = *row;
            }
            let _ = command_without_settings_update(self.device, 0x030b, &args, None);
        }
    }

    pub fn set_lid_logo(&mut self, mode: LidLogoMode) {
        if mode == LidLogoMode::Off {
            let _ = command(self.device, 0x0300, &[1, 4, 0], None);
        } else {
            let _ = command(self.device, 0x0300, &[1, 4, 1], None);
            if mode == LidLogoMode::On {
                let _ = command(self.device, 0x0302, &[1, 4, 0], None);
            } else {
                let _ = command(self.device, 0x0302, &[1, 4, 1], None);
            }
        }
        app(OsdEvent::LidLogo(mode).into());
        self.persist_config();
    }

    pub fn init_keyboard_width(&mut self) {
        let mut width = 18;
        loop {
            if width < 1 {
                return;
            }
            if command(self.device, 0x030b, &[255, 0, 0, width], None).is_ok() {
                self.app_config.keyboard_width = width;
                return;
            } else {
                width -= 1;
            }
        }
    }


    pub fn keyboard_control(&self, state: bool) {
        // Select Chroma-RGB so Windows Dynamic Lighting cannot take control.
        let _ = command(self.device, 0x0f10, &[1], None);
        let arg = if state { 3 } else { 0 };
        let _ = command(self.device, 0x0004, &[arg, 0], None);
    }

    pub fn enable_multimedia_keys(&self) {
        let _ = command(self.device, 0x0206, &[0, 1], None);
    }

    pub fn restore_fn_keys(&self) {
        let _ = command(self.device, 0x0206, &[0, 0], None);
    }


    fn persist_config(&mut self) {
        persist_config(self.app_config, self.persist_buffer);
    }
}

fn keyboard_color_args(width: u8, r: u8, g: u8, b: u8) -> Vec<u8> {
    vec![
        255, 0, 0, width, 0, 0, 0, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b,
        r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b, r, g, b,
        r, g, b,
    ]
}

/// The same frame as `keyboard_color_args`, but with a distinct colour per
/// column: `[255, row, start, stop]` followed by one triplet per column.
///
/// Every column including column 0 gets a colour. The stop column is inclusive,
/// so it is `len - 1`, and it is taken from the colours supplied rather than
/// from the configured width so the frame always describes exactly what it
/// carries.
fn keyboard_row_color_args(colors: &[ThemeColor]) -> Vec<u8> {
    let mut args = Vec::with_capacity(4 + colors.len() * 3);
    args.extend([255, 0, 0, colors.len().saturating_sub(1) as u8]);
    for color in colors {
        args.extend([color.r, color.g, color.b]);
    }
    args
}

fn next_keyboard_brightness(current: u8, up: bool) -> u8 {
    let level_discrete = (current as f64 / 51.0).round() as i32;
    let change = if up { 1 } else { -1 };
    (level_discrete + change).clamp(0, 5) as u8 * 51
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_color_args_fit_external_packet_payload() {
        let args = keyboard_color_args(18, 1, 2, 3);

        assert!(args.len() <= HID_PACKET_ARGS_LEN);
    }

    #[test]
    fn keyboard_color_args_preserve_header_and_repeated_rgb_payload() {
        let args = keyboard_color_args(18, 1, 2, 3);

        assert_eq!(&args[..7], &[255, 0, 0, 18, 0, 0, 0]);
        assert!(args[7..].chunks_exact(3).all(|chunk| chunk == [1, 2, 3]));
    }

    #[test]
    fn keyboard_row_color_args_fill_the_same_frame_as_a_broadcast() {
        let colors = std::array::from_fn::<_, MATRIX_COLUMNS, _>(|i| {
            ThemeColor::new(i as u8, 100 + i as u8, 200 + i as u8)
        });
        let args = keyboard_row_color_args(&colors);

        // A full-width per-column frame is the same size as a broadcast one.
        assert_eq!(args.len(), keyboard_color_args(18, 1, 2, 3).len());
        assert!(args.len() <= HID_PACKET_ARGS_LEN);
        // The stop column is inclusive, so a 19-column frame stops at 18.
        assert_eq!(&args[..4], &[255, 0, 0, (MATRIX_COLUMNS - 1) as u8]);
    }

    #[test]
    fn keyboard_row_color_args_paint_column_zero() {
        // The broadcast builder hardwires column 0 dark. A per-column effect
        // must own every column, or a mirrored one ends up half a column off
        // centre with the leftmost key permanently unlit.
        let colors = [ThemeColor::new(11, 22, 33); 4];
        let args = keyboard_row_color_args(&colors);

        assert_eq!(&args[4..7], &[11, 22, 33]);
    }

    #[test]
    fn keyboard_row_color_args_place_each_column_in_order() {
        let colors = std::array::from_fn::<_, MATRIX_COLUMNS, _>(|i| {
            ThemeColor::new(i as u8, 100 + i as u8, 200 + i as u8)
        });
        let args = keyboard_row_color_args(&colors);

        for (index, color) in colors.iter().enumerate() {
            let offset = 4 + index * 3;
            assert_eq!(
                &args[offset..offset + 3],
                &[color.r, color.g, color.b],
                "column {index} landed in the wrong triplet"
            );
        }
    }

    #[test]
    fn keyboard_row_color_args_describe_a_narrow_keyboard() {
        // A model that probed narrower than the protocol maximum must get a
        // frame that stops where its LEDs do, not one padded out to 18.
        let colors = vec![ThemeColor::new(9, 9, 9); 14];
        let args = keyboard_row_color_args(&colors);

        assert_eq!(args[3], 13, "the stop column is inclusive");
        assert_eq!(args.len(), 4 + 14 * 3);
        assert!(args.len() <= HID_PACKET_ARGS_LEN);
    }

    #[test]
    fn keyboard_brightness_steps_clamp_at_edges() {
        assert_eq!(next_keyboard_brightness(0, false), 0);
        assert_eq!(next_keyboard_brightness(255, true), 255);
    }

    #[test]
    fn keyboard_brightness_steps_by_fifty_one() {
        assert_eq!(next_keyboard_brightness(102, true), 153);
        assert_eq!(next_keyboard_brightness(102, false), 51);
    }
}
