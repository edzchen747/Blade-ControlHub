use crate::{
    razer::{
        device_handle::device,
        device_state::{PerfMode, RGBEffect},
    },
    ui::icon,
    win::startup::Startup,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsdIconId {
    Brightness,
    KeyboardBrightness,
    MicMute(bool),
    Trackpad(bool),
    RGBEffect,
    UnderGlow(bool),
    RefreshRate,
}

pub enum AppEvent {
    ScreenBrightness(u8),
    KeyboardBrightness(u8),
    PerfMode(PerfMode),
    MicMute(bool),
    Trackpad(bool),
    RGBEffect(RGBEffect),
    UnderGlow(u8),
    RefreshRate(u32, u8, u8),
    Quit,
    Restart,
    StartupToggle(bool),
}

pub fn process_event(
    event: AppEvent,
    tray_icon: &mut tray_icon::TrayIcon,
    osd_text: &mut String,
    osd_icon_id: &mut Option<OsdIconId>,
    osd_total_levels: &mut u8,
    osd_curr_level: &mut u8,
) -> bool {
    let (trigger_osd, text, icon_id, total_levels, curr_level): (
        bool,
        String,
        Option<OsdIconId>,
        u8,
        u8,
    ) = match event {
        AppEvent::ScreenBrightness(lvl) => (
            true,
            "".to_string(),
            Some(OsdIconId::Brightness),
            10,
            lvl / 10,
        ),
        AppEvent::KeyboardBrightness(lvl) => (
            true,
            "".to_string(),
            Some(OsdIconId::KeyboardBrightness),
            5,
            lvl / 51,
        ),
        AppEvent::PerfMode(mode) => {
            icon::set_perf_mode_icon(tray_icon, mode);
            (true, mode.to_string(), None, 0, 0)
        }
        AppEvent::MicMute(muted) => (
            true,
            "".to_string(),
            Some(OsdIconId::MicMute(muted)),
            1,
            !muted as u8,
        ),
        AppEvent::Trackpad(state) => (
            true,
            "".to_string(),
            Some(OsdIconId::Trackpad(state)),
            1,
            state as u8,
        ),
        AppEvent::RGBEffect(effect) => (true, effect.to_string(), Some(OsdIconId::RGBEffect), 0, 0),
        AppEvent::UnderGlow(lvl) => (
            true,
            "".to_string(),
            Some(OsdIconId::UnderGlow(lvl > 0)),
            1,
            lvl / 255,
        ),
        AppEvent::RefreshRate(current, level, total) => (
            true,
            current.to_string(),
            Some(OsdIconId::RefreshRate),
            total,
            level,
        ),
        AppEvent::Quit => {
            device().shutdown();
            std::process::exit(0);
        }
        AppEvent::Restart => restart_app(0),
        AppEvent::StartupToggle(enabled) => {
            if enabled {
                if !Startup::is_registered() {
                    Startup::register();
                }
            } else {
                if Startup::is_registered() {
                    Startup::unregister();
                }
            }
            (false, "".to_string(), None, 0, 0)
        }
    };
    if trigger_osd {
        osd_text.clear();
        osd_text.push_str(&text);
        *osd_icon_id = icon_id;
        *osd_total_levels = total_levels;
        *osd_curr_level = curr_level;
    }
    trigger_osd
}

pub fn get_icon_data(id: &OsdIconId) -> (&'static str, &'static [u8]) {
    match id {
        OsdIconId::Brightness => (
            "bytes://brightness.svg",
            include_bytes!("../../assets/brightness.svg"),
        ),
        OsdIconId::KeyboardBrightness => (
            "bytes://keyboard.svg",
            include_bytes!("../../assets/keyboard.svg"),
        ),
        OsdIconId::MicMute(false) => ("bytes://mic.svg", include_bytes!("../../assets/mic.svg")),
        OsdIconId::MicMute(true) => (
            "bytes://mic_off.svg",
            include_bytes!("../../assets/mic_off.svg"),
        ),
        OsdIconId::Trackpad(true) => (
            "bytes://trackpad.svg",
            include_bytes!("../../assets/trackpad.svg"),
        ),
        OsdIconId::Trackpad(false) => (
            "bytes://trackpad_off.svg",
            include_bytes!("../../assets/trackpad_off.svg"),
        ),
        OsdIconId::RGBEffect => (
            "bytes://rgb_effect.svg",
            include_bytes!("../../assets/rgb_effect.svg"),
        ),
        OsdIconId::UnderGlow(true) => (
            "bytes://underglow.svg",
            include_bytes!("../../assets/underglow.svg"),
        ),
        OsdIconId::UnderGlow(false) => (
            "bytes://underglow_off.svg",
            include_bytes!("../../assets/underglow_off.svg"),
        ),
        OsdIconId::RefreshRate => (
            "bytes://refresh.svg",
            include_bytes!("../../assets/refresh.svg"),
        ),
    }
}

pub fn restart_app(code: i32) -> ! {
    let current_exe = std::env::current_exe().expect("Failed to get current exe path");
    std::process::Command::new(current_exe)
        .spawn()
        .expect("Failed to restart");
    std::process::exit(code);
}
