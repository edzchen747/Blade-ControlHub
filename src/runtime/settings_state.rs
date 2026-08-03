use serde::{Deserialize, Serialize};

use crate::config::ThemeColor;
use crate::razer::{
    config::{AppConfig, CustomModeConfig, DeviceState, FanSpeedLimits, FanSpeeds, PowerProfile},
    enums::{BATTERY_LIMITS, BatteryLimit, PerfMode, RGBEffect},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceProfileState {
    pub keyboard_brightness: u8,
    pub rgb_effect: RGBEffect,
    pub rgb_effects: Vec<RGBEffect>,
    pub underglow_enabled: bool,
    pub perf_mode: PerfMode,
    pub perf_modes: Vec<PerfMode>,
    pub fan_speeds: FanSpeeds,
    pub refresh_rate: u32,
    pub supported_refresh_rates: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsState {
    pub model_name: String,
    pub current_profile: PowerProfile,
    pub ac_profile: DeviceProfileState,
    pub battery_profile: DeviceProfileState,
    pub custom_mode_config: CustomModeConfig,
    pub battery_limit: BatteryLimit,
    pub battery_limits: Vec<BatteryLimit>,
    pub fan_speed_limits: FanSpeedLimits,
    pub primary_multimedia_keys: bool,
    pub theme_color: ThemeColor,
}

impl SettingsState {
    pub fn from_config(config: AppConfig, supported_refresh_rates: Vec<u32>) -> Self {
        Self::from_config_with_fan_speed_limits(
            config,
            supported_refresh_rates,
            FanSpeedLimits::default(),
        )
    }

    pub fn from_config_with_fan_speed_limits(
        config: AppConfig,
        supported_refresh_rates: Vec<u32>,
        fan_speed_limits: FanSpeedLimits,
    ) -> Self {
        Self::from_config_with_fan_speed_limits_and_battery_limit(
            config,
            supported_refresh_rates,
            fan_speed_limits,
            BatteryLimit::Off,
        )
    }

    pub fn from_config_with_fan_speed_limits_and_battery_limit(
        config: AppConfig,
        supported_refresh_rates: Vec<u32>,
        fan_speed_limits: FanSpeedLimits,
        battery_limit: BatteryLimit,
    ) -> Self {
        Self {
            model_name: config.model_name.clone(),
            current_profile: AppConfig::active_profile(),
            ac_profile: DeviceProfileState::from_device_state(
                config.profile(PowerProfile::Ac),
                supported_refresh_rates.clone(),
            ),
            battery_profile: DeviceProfileState::from_device_state(
                config.profile(PowerProfile::Battery),
                supported_refresh_rates,
            ),
            custom_mode_config: config.custom_mode_config.clone(),
            battery_limit,
            battery_limits: BATTERY_LIMITS.to_vec(),
            fan_speed_limits,
            primary_multimedia_keys: config.primary_multimedia_keys,
            theme_color: config.theme_color,
        }
    }

    pub fn profile(&self, profile: PowerProfile) -> &DeviceProfileState {
        match profile {
            PowerProfile::Ac => &self.ac_profile,
            PowerProfile::Battery => &self.battery_profile,
        }
    }

    pub fn profile_mut(&mut self, profile: PowerProfile) -> &mut DeviceProfileState {
        match profile {
            PowerProfile::Ac => &mut self.ac_profile,
            PowerProfile::Battery => &mut self.battery_profile,
        }
    }
}

impl DeviceProfileState {
    fn from_device_state(state: DeviceState, supported_refresh_rates: Vec<u32>) -> Self {
        let mut rgb_effect = state.rgb_effect.clone();
        let mut perf_mode = state.perf_mode.clone();
        Self {
            keyboard_brightness: state.key_lvl,
            rgb_effect: rgb_effect.value(),
            rgb_effects: state.rgb_effect.items,
            underglow_enabled: state.vc_lvl > 0,
            perf_mode: perf_mode.value(),
            perf_modes: state.perf_mode.items,
            fan_speeds: state.fan_speeds,
            refresh_rate: state.screen_refresh,
            supported_refresh_rates,
        }
    }
}

impl From<AppConfig> for SettingsState {
    fn from(config: AppConfig) -> Self {
        Self::from_config(config, Vec::new())
    }
}

impl Default for DeviceProfileState {
    fn default() -> Self {
        Self::from_device_state(DeviceState::default(), Vec::new())
    }
}

impl Default for SettingsState {
    fn default() -> Self {
        AppConfig::default().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_state_contains_both_profiles_and_globals() {
        let mut config = AppConfig::default();
        config.profile_mut(PowerProfile::Ac).key_lvl = 204;
        config.profile_mut(PowerProfile::Battery).key_lvl = 51;
        config.primary_multimedia_keys = true;

        let state = SettingsState::from_config(config, vec![60, 240]);

        assert_eq!(state.ac_profile.keyboard_brightness, 204);
        assert_eq!(state.battery_profile.keyboard_brightness, 51);
        assert_eq!(state.ac_profile.supported_refresh_rates, vec![60, 240]);
        assert_eq!(state.battery_profile.supported_refresh_rates, vec![60, 240]);
        assert_eq!(state.fan_speed_limits, FanSpeedLimits::default());
        assert!(state.primary_multimedia_keys);
        assert_eq!(state.theme_color, ThemeColor::default());
    }

    #[test]
    fn settings_state_keeps_firmware_fan_speed_limits() {
        let limits = FanSpeedLimits { min: 20, max: 30 };

        let state = SettingsState::from_config_with_fan_speed_limits(
            AppConfig::default(),
            Vec::new(),
            limits,
        );

        assert_eq!(state.fan_speed_limits, limits);
    }

    #[test]
    fn settings_state_uses_the_device_battery_limit() {
        let state = SettingsState::from_config_with_fan_speed_limits_and_battery_limit(
            AppConfig::default(),
            Vec::new(),
            FanSpeedLimits::default(),
            BatteryLimit::Limit65,
        );

        assert_eq!(state.battery_limit, BatteryLimit::Limit65);
        assert_eq!(state.battery_limits, BATTERY_LIMITS);
    }
}
