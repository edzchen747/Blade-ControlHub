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
