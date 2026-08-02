#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SimpleMonitor {
    width: u32,
    height: u32,
}

fn settings_spawn_position(monitor: SimpleMonitor, window_size: egui::Vec2) -> egui::Pos2 {
    let screen_width = monitor.width as f32;
    let screen_height = monitor.height as f32;
    let padding = screen_height * SETTINGS_PADDING_RATIO;

    egui::pos2(
        screen_width - window_size.x - padding * 0.1,
        screen_height - window_size.y - padding,
    )
}

fn pre_resolve_monitor_dimensions() -> Option<SimpleMonitor> {
    let (width, height) = primary_screen_dimensions();
    let dpi = primary_screen_dpi();
    monitor_from_dimensions(
        logical_screen_dimension(width, dpi),
        logical_screen_dimension(height, dpi),
    )
}

fn monitor_from_dimensions(width: i32, height: i32) -> Option<SimpleMonitor> {
    if width > 0 && height > 0 {
        Some(SimpleMonitor {
            width: width as u32,
            height: height as u32,
        })
    } else {
        None
    }
}

fn primary_screen_dimensions() -> (i32, i32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    // SAFETY: GetSystemMetrics is a side-effect-free Win32 query for process
    // desktop metrics and accepts these constant indexes.
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

fn primary_screen_dpi() -> u32 {
    use windows::Win32::UI::HiDpi::GetDpiForSystem;

    // SAFETY: GetDpiForSystem reads the current process's system DPI setting.
    unsafe { GetDpiForSystem().max(WINDOWS_DEFAULT_DPI) }
}

fn logical_screen_dimension(physical_dimension: i32, dpi: u32) -> i32 {
    let scale_factor = dpi.max(WINDOWS_DEFAULT_DPI) as f32 / WINDOWS_DEFAULT_DPI as f32;
    (physical_dimension as f32 / scale_factor) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_spawn_position_uses_bottom_right_padding() {
        let monitor = SimpleMonitor {
            width: 1920,
            height: 1080,
        };
        let position = settings_spawn_position(monitor, egui::vec2(453.0, 631.0));

        assert_eq!(
            position,
            egui::pos2(
                1920.0 - 453.0 - (1080.0 * SETTINGS_PADDING_RATIO * 0.1),
                1080.0 - 631.0 - (1080.0 * SETTINGS_PADDING_RATIO),
            )
        );
    }

    #[test]
    fn invalid_monitor_dimensions_are_ignored() {
        assert_eq!(monitor_from_dimensions(0, 1080), None);
        assert_eq!(monitor_from_dimensions(1920, 0), None);
        assert_eq!(monitor_from_dimensions(-1, 1080), None);
    }

    #[test]
    fn physical_monitor_dimensions_are_scaled_to_egui_points() {
        assert_eq!(logical_screen_dimension(2560, 144), 1706);
        assert_eq!(logical_screen_dimension(1600, 144), 1066);
    }

    #[test]
    fn settings_app_starts_loading_without_default_settings_state() {
        let app = SettingsApp::new(None);

        assert!(!app.state_loaded);
        assert!(
            app.settings
                .with_settings(|settings| settings.state.is_none() && settings.show)
        );
    }

    #[test]
    fn settings_app_uses_a_reactive_update_worker_after_initial_load() {
        let mut app = SettingsApp::new(Some(SettingsState::default()));
        app.start_settings_update_worker(egui::Context::default());

        assert!(app.settings_update_thread.is_some());
    }

    #[test]
    fn failed_perf_mode_update_flashes_unsupported_notice() {
        let mut app = SettingsApp::new(Some(SettingsState::default()));
        let ctx = egui::Context::default();

        app.handle_failed_perf_mode_update(PerfMode::Performance, &ctx);

        assert_eq!(
            app.settings
                .with_settings(|settings| settings.unsupported_perf_mode_message()),
            Some("\"Performance\" mode not supported".to_string())
        );
    }

    #[test]
    fn razer_key_capture_poll_interval_uses_settings_listen_interval() {
        assert_eq!(
            razer_key_capture_poll_interval(),
            Duration::from_millis(SETTINGS_KEY_LISTEN_INTERVAL_MS)
        );
    }

    #[test]
    fn positive_monitor_dimensions_are_preserved() {
        assert_eq!(
            monitor_from_dimensions(1920, 1080),
            Some(SimpleMonitor {
                width: 1920,
                height: 1080,
            })
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_icon_conversion_builds_alpha_mask_and_bgra_pixels() {
        let rgba = vec![
            0x11, 0x22, 0x33, 0xff, //
            0xaa, 0xbb, 0xcc, 0x00,
        ];

        let (and_mask, bgra) =
            windows_icon_masks_and_bgra(rgba, 2, 1).expect("valid icon data should convert");

        assert_eq!(and_mask, vec![0x00, 0x01]);
        assert_eq!(
            bgra,
            vec![
                0x33, 0x22, 0x11, 0xff, //
                0xcc, 0xbb, 0xaa, 0x00,
            ]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_icon_conversion_rejects_wrong_buffer_length() {
        assert!(windows_icon_masks_and_bgra(vec![0xff, 0x00, 0x00], 1, 1).is_none());
    }
}
