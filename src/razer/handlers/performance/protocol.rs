fn fan_speed_command_args(fan: u8, speed: u8) -> [u8; 4] {
    [1, fan, speed, 0]
}

fn performance_mode_command_args(perf_mode: PerfMode, fan_speed: u8) -> [u8; 4] {
    let fan_speed_mode = if fan_speed == 0 {
        AUTO_FAN_SPEED_MODE
    } else {
        FIXED_FAN_SPEED_MODE
    };
    [1, 0, perf_mode as u8, fan_speed_mode]
}

fn performance_mode_query_args() -> [u8; 4] {
    [0, 0, 0, 0]
}

fn custom_mode_config_command_args(component: u8, level: u8) -> [u8; 4] {
    [1, component, level, 0]
}

#[cfg(test)]
mod tests {
    use super::{
        custom_mode_config_command_args, fan_speed_command_args, performance_mode_command_args,
        performance_mode_query_args, validate_custom_mode_level, validate_fan_speed,
    };
    use crate::razer::{config::FanSpeedLimits, enums::PerfMode};

    #[test]
    fn fan_speed_validation_accepts_auto_and_manual_bounds() {
        for speed in [0, 10, 46] {
            assert!(validate_fan_speed(speed, FanSpeedLimits::default()).is_ok());
        }
    }

    #[test]
    fn fan_speed_validation_rejects_values_outside_supported_range() {
        for speed in [1, 9, 47, u8::MAX] {
            assert!(validate_fan_speed(speed, FanSpeedLimits::default()).is_err());
        }
    }

    #[test]
    fn fan_speed_validation_uses_firmware_limits() {
        let limits = FanSpeedLimits { min: 20, max: 30 };

        assert!(validate_fan_speed(20, limits).is_ok());
        assert!(validate_fan_speed(30, limits).is_ok());
        assert!(validate_fan_speed(19, limits).is_err());
        assert!(validate_fan_speed(31, limits).is_err());
    }

    #[test]
    fn fan_speed_commands_always_use_a_zero_mode_byte() {
        assert_eq!(fan_speed_command_args(1, 0), [1, 1, 0, 0]);
        assert_eq!(fan_speed_command_args(4, 46), [1, 4, 46, 0]);
    }

    #[test]
    fn performance_mode_command_carries_the_fan_speed_mode() {
        assert_eq!(
            performance_mode_command_args(PerfMode::Balanced, 0),
            [1, 0, PerfMode::Balanced as u8, 0]
        );
        assert_eq!(
            performance_mode_command_args(PerfMode::Balanced, 46),
            [1, 0, PerfMode::Balanced as u8, 1]
        );
    }

    #[test]
    fn performance_mode_query_uses_the_global_zone() {
        assert_eq!(performance_mode_query_args(), [0, 0, 0, 0]);
    }

    #[test]
    fn custom_mode_levels_are_limited_to_firmware_range() {
        for level in 0..=3 {
            assert!(validate_custom_mode_level(level).is_ok());
        }
        assert!(validate_custom_mode_level(4).is_err());
        assert!(validate_custom_mode_level(u8::MAX).is_err());
    }

    #[test]
    fn custom_mode_commands_target_cpu_and_gpu_with_the_selected_level() {
        assert_eq!(custom_mode_config_command_args(1, 0), [1, 1, 0, 0]);
        assert_eq!(custom_mode_config_command_args(2, 3), [1, 2, 3, 0]);
    }
}
