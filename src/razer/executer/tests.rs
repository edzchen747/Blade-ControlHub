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
}
