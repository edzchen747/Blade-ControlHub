fn selected_profile_state(settings: &Settings) -> Option<DeviceProfileState> {
    settings
        .state
        .as_ref()
        .map(|state| state.profile(settings.selected_profile).clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PerfModeClickAction {
    None,
    Set,
    UnsupportedNotice,
}

fn perf_mode_click_action(
    profile_state: &DeviceProfileState,
    mode: PerfMode,
) -> PerfModeClickAction {
    if !profile_state.perf_modes.contains(&mode) {
        PerfModeClickAction::UnsupportedNotice
    } else if mode == profile_state.perf_mode {
        PerfModeClickAction::None
    } else {
        PerfModeClickAction::Set
    }
}

fn handle_perf_mode_click(
    settings: &mut Settings,
    ctx: &egui::Context,
    profile: PowerProfile,
    profile_state: &DeviceProfileState,
    mode: PerfMode,
) {
    match perf_mode_click_action(profile_state, mode) {
        PerfModeClickAction::None => {}
        PerfModeClickAction::Set => {
            set_perf_mode(settings, profile, mode);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        PerfModeClickAction::UnsupportedNotice => {
            settings.flash_unsupported_perf_mode(mode);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            ctx.request_repaint_after(Settings::unsupported_perf_mode_notice_duration());
        }
    }
}

fn set_perf_mode(settings: &mut Settings, profile: PowerProfile, mode: PerfMode) {
    if let Some(state) = settings.state.as_mut() {
        state.profile_mut(profile).perf_mode = mode;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetPerfMode(profile, mode));
    }
}

fn set_custom_mode_config(settings: &mut Settings, changed: CustomModeSetting, level: u8) {
    if let Some(state) = settings.state.as_mut() {
        match changed {
            CustomModeSetting::Cpu => state.custom_mode_config.cpu_level = level,
            CustomModeSetting::Gpu => state.custom_mode_config.gpu_level = level,
        }
        let cpu_level = state.custom_mode_config.cpu_level;
        let gpu_level = state.custom_mode_config.gpu_level;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetCustomModeConfig {
            cpu_level,
            gpu_level,
        });
    }
}

fn set_fan_speed(settings: &mut Settings, profile: PowerProfile, speed: u8) {
    if let Some(state) = settings.state.as_mut() {
        let profile_state = state.profile_mut(profile);
        profile_state.fan_speeds.set(profile_state.perf_mode, speed);
        settings.update = true;
        settings.queue_command(SettingsCommand::SetFanSpeed(profile, speed));
    }
}

fn set_refresh_rate(settings: &mut Settings, profile: PowerProfile, hz: u32) {
    if let Some(state) = settings.state.as_mut() {
        state.profile_mut(profile).refresh_rate = hz;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetRefreshRate(profile, hz));
    }
}

fn set_keyboard_brightness(settings: &mut Settings, profile: PowerProfile, brightness: u8) {
    if let Some(state) = settings.state.as_mut() {
        state.profile_mut(profile).keyboard_brightness = brightness;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetKeyboardBrightness(profile, brightness));
    }
}

fn set_rgb_effect(settings: &mut Settings, profile: PowerProfile, effect: RGBEffect) {
    if let Some(state) = settings.state.as_mut() {
        state.profile_mut(profile).rgb_effect = effect;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetRgbEffect(profile, effect));
    }
}

fn set_under_glow(settings: &mut Settings, profile: PowerProfile, enabled: bool) {
    if let Some(state) = settings.state.as_mut() {
        state.profile_mut(profile).underglow_enabled = enabled;
        settings.update = true;
        settings.queue_command(SettingsCommand::SetUnderGlow(profile, enabled));
    }
}


include!("commands_tests.rs");
