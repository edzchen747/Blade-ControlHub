#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_lock() -> MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn perf_mode_color_maps_known_modes() {
        let _guard = test_lock();

        assert_eq!(perf_mode_hex_color(PerfMode::Balanced), "#FFD600");
        assert_eq!(perf_mode_hex_color(PerfMode::Turbo), "#D50000");
        assert_eq!(perf_mode_hex_color(PerfMode::BatterySaver), "#9BF542");
    }

    #[test]
    fn perf_mode_color_uses_default_for_unknown() {
        let _guard = test_lock();

        assert_eq!(perf_mode_hex_color(PerfMode::Unknown), DEFAULT_ICON_COLOR);
    }

    #[test]
    fn tray_shutdown_sets_stop_flag_and_clears_sender() {
        let _guard = test_lock();

        let (tx, _rx) = channel();
        *tray_update_sender() = Some(tx);
        TRAY_SHUTDOWN.store(false, Ordering::SeqCst);

        TrayManager::shutdown();

        assert!(TRAY_SHUTDOWN.load(Ordering::SeqCst));
        assert!(tray_update_sender().is_none());
    }

    #[test]
    fn reset_tray_state_clears_thread_id_and_initialization() {
        let _guard = test_lock();

        TRAY_INITIALIZED.store(true, Ordering::SeqCst);
        TRAY_SHUTDOWN.store(false, Ordering::SeqCst);
        TRAY_THREAD_ID.store(42, Ordering::SeqCst);

        reset_tray_state();

        assert!(!TRAY_INITIALIZED.load(Ordering::SeqCst));
        assert!(TRAY_SHUTDOWN.load(Ordering::SeqCst));
        assert_eq!(TRAY_THREAD_ID.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn join_tray_icon_thread_drains_handle() {
        let _guard = test_lock();

        *tray_icon_thread() = Some(thread::spawn(|| {}));

        join_tray_icon_thread();

        assert!(tray_icon_thread().is_none());
    }

    #[test]
    fn join_tray_click_thread_drains_handle() {
        let _guard = test_lock();

        *tray_click_thread() = Some(thread::spawn(|| {}));

        join_tray_click_thread();

        assert!(tray_click_thread().is_none());
    }
}
