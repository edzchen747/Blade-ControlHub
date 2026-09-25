#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_query_runs_on_the_first_snapshot_and_every_period_queries() {
        assert!(should_query_battery_limit(1), "first snapshot must query");
        for queries in 2..SETTINGS_STATE_BATTERY_QUERY_PERIOD {
            assert!(!should_query_battery_limit(queries), "queries={queries}");
        }
        assert!(should_query_battery_limit(SETTINGS_STATE_BATTERY_QUERY_PERIOD));
        assert!(!should_query_battery_limit(SETTINGS_STATE_BATTERY_QUERY_PERIOD + 1));
        assert!(should_query_battery_limit(2 * SETTINGS_STATE_BATTERY_QUERY_PERIOD));
    }

    #[test]
    fn battery_query_skips_zero_queries() {
        assert!(!should_query_battery_limit(0));
    }

    /// `record_battery_limit` resets the counter to zero after a write, and a
    /// snapshot increments before testing it, so the next snapshot re-reads the
    /// device. Without that, the window reads back the cached pre-change value
    /// and snaps the control away from what the user just picked.
    #[test]
    fn a_write_makes_the_next_snapshot_re_read_the_device() {
        let counter_after_write = 0;

        assert!(should_query_battery_limit(counter_after_write + 1));
    }

    /// The window's controls show their own new value, so the overlay they
    /// would raise is noise. Everything else — a Razer special key, an Fn
    /// combination, a charger being plugged in — has nothing else to show for
    /// itself and keeps its overlay even while the window is open and focused.
    #[test]
    fn only_the_settings_window_commands_suppress_the_osd() {
        let (tx, _rx) = std::sync::mpsc::channel();
        assert!(window_originated(&DeviceCmd::SetKeyboardBrightness(
            PowerProfile::Ac,
            204,
            tx
        )));

        let (tx, _rx) = std::sync::mpsc::channel();
        assert!(window_originated(&DeviceCmd::SetPerfMode(
            PowerProfile::Ac,
            PerfMode::Turbo,
            tx
        )));

        let (tx, _rx) = std::sync::mpsc::channel();
        assert!(window_originated(&DeviceCmd::SetBatteryLimit(
            BatteryLimit::Limit80,
            tx
        )));

        assert!(
            !window_originated(&DeviceCmd::AdjustKeyboardLight(true)),
            "Fn+F11 must still raise the backlight overlay"
        );
        assert!(
            !window_originated(&DeviceCmd::CyclePerfMode),
            "the performance key must still raise its overlay"
        );
        assert!(
            !window_originated(&DeviceCmd::CycleRGBMode),
            "the Copilot key must still raise its overlay"
        );
        assert!(!window_originated(&DeviceCmd::CycleRefreshRate));
        assert!(!window_originated(&DeviceCmd::CycleBatteryLimit));
        assert!(!window_originated(&DeviceCmd::ToggleUnderGlow));
        assert!(!window_originated(&DeviceCmd::AdjustScreenBrightness(-10)));
    }

    #[test]
    fn capture_drain_discards_commands_but_keeps_shutdown() {
        let (tx, _rx) = std::sync::mpsc::channel();
        assert!(should_discard_during_capture(&DeviceCmd::CycleRGBMode));
        assert!(should_discard_during_capture(&DeviceCmd::AdjustKeyboardLight(true)));
        assert!(should_discard_during_capture(&DeviceCmd::SetKeyboardColor(1, 2, 3, 4)));
        assert!(!should_discard_during_capture(&DeviceCmd::Shutdown(tx)));
    }
}
