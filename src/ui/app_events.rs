use eframe::egui;

use crate::{
    razer::{
        device_handle::device,
        device_state::{PerfMode, RGBEffect},
    },
    ui::icon,
};

pub enum AppEvent {
    ScreenBrightness(u8),
    KeyboardBrightness(u8),
    PerfMode(PerfMode),
    MicMute(bool),
    Trackpad(bool),
    RGBEffect(RGBEffect),
    UnderGlow(u8),
    Quit,
    Restart,
}

pub fn process_event(
    event: AppEvent,
    tray_icon: &mut tray_icon::TrayIcon,
    osd_text: &mut String,
    osd_icon: &mut Option<egui::Image<'static>>,
    osd_total_levels: &mut u8,
    osd_curr_level: &mut u8,
) -> bool {
    let (trigger_osd, text, icon, total_levels, curr_level): (
        bool,
        String,
        Option<egui::Image<'static>>,
        u8,
        u8,
    ) = match event {
        AppEvent::ScreenBrightness(lvl) => {
            let icon = egui::Image::from_bytes(
                "bytes://brightness.svg",
                include_bytes!("../../assets/brightness.svg"),
            );
            (true, "".to_string(), Some(icon), 10, lvl / 10)
        }
        AppEvent::KeyboardBrightness(lvl) => {
            let icon = egui::Image::from_bytes(
                "bytes://keyboard.svg",
                include_bytes!("../../assets/keyboard.svg"),
            );
            (true, "".to_string(), Some(icon), 5, lvl / 51)
        }
        AppEvent::PerfMode(mode) => {
            icon::set_perf_mode_icon(tray_icon, mode);
            (true, mode.to_string(), None, 0, 0)
        }
        AppEvent::MicMute(muted) => {
            let mic_icon = match muted {
                true => egui::Image::from_bytes(
                    "bytes://mic_off.svg",
                    include_bytes!("../../assets/mic_off.svg"),
                ),
                false => egui::Image::from_bytes(
                    "bytes://mic.svg",
                    include_bytes!("../../assets/mic.svg"),
                ),
            };
            (true, "".to_string(), Some(mic_icon), 1, !muted as u8)
        }
        AppEvent::Trackpad(state) => {
            let trackpad_icon = match state {
                true => egui::Image::from_bytes(
                    "bytes://trackpad.svg",
                    include_bytes!("../../assets/trackpad.svg"),
                ),
                false => egui::Image::from_bytes(
                    "bytes://trackpad_off.svg",
                    include_bytes!("../../assets/trackpad_off.svg"),
                ),
            };
            (true, "".to_string(), Some(trackpad_icon), 1, state as u8)
        }
        AppEvent::RGBEffect(effect) => {
            let icon = egui::Image::from_bytes(
                "bytes://rgb_effect.svg",
                include_bytes!("../../assets/rgb_effect.svg"),
            );
            (true, effect.to_string(), Some(icon), 0, 0)
        }
        AppEvent::UnderGlow(lvl) => {
            let under_glow_icon = if lvl > 0 {
                egui::Image::from_bytes(
                    "bytes://underglow.svg",
                    include_bytes!("../../assets/underglow.svg"),
                )
            } else {
                egui::Image::from_bytes(
                    "bytes://underglow_off.svg",
                    include_bytes!("../../assets/underglow_off.svg"),
                )
            };
            (true, "".to_string(), Some(under_glow_icon), 1, lvl / 255)
        }
        AppEvent::Quit => {
            device().shutdown();
            std::process::exit(0);
        }
        AppEvent::Restart => restart_app(0),
        _ => (false, "".to_string(), None, 0, 0),
    };
    if trigger_osd {
        osd_text.clear();
        osd_text.push_str(&text.to_string());
        *osd_icon = icon;
        *osd_total_levels = total_levels;
        *osd_curr_level = curr_level;
    }
    trigger_osd
}

pub fn restart_app(code: i32) -> ! {
    let current_exe = std::env::current_exe().expect("Failed to get current exe path");
    std::process::Command::new(current_exe)
        .spawn()
        .expect("Failed to restart");
    std::process::exit(code);
}
