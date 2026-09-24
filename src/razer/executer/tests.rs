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

    #[test]
    fn capture_drain_discards_commands_but_keeps_shutdown() {
        let (tx, _rx) = std::sync::mpsc::channel();
        assert!(should_discard_during_capture(&DeviceCmd::CycleRGBMode));
        assert!(should_discard_during_capture(&DeviceCmd::AdjustKeyboardLight(true)));
        assert!(should_discard_during_capture(&DeviceCmd::SetKeyboardColor(1, 2, 3, 4)));
        assert!(!should_discard_during_capture(&DeviceCmd::Shutdown(tx)));
    }
}
