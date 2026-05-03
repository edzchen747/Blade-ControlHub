use crate::razer::device_handle::device;
use crate::razer::device_state::PerfMode;
use crate::ui::app_events::{AppEvent, restart_app};
use crate::ui::tray_app::{TRAY_APP_TX, tray_app};

use resvg::{tiny_skia, usvg};
use std::thread;
use tray_icon::TrayIconEvent;
use tray_icon::{
    Icon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

// Use to reload instead of panicking
pub trait OptionReload<T> {
    fn or_reload(self, msg: &str) -> T;
}

impl<T> OptionReload<T> for Option<T> {
    fn or_reload(self, msg: &str) -> T {
        self.unwrap_or_else(|| {
            eprintln!("Fatal internal error: {msg}");
            restart_app(1);
        })
    }
}

pub trait ResultReload<T, E> {
    fn or_reload(self, msg: &str) -> T;
}

impl<T, E: std::fmt::Debug> ResultReload<T, E> for Result<T, E> {
    fn or_reload(self, msg: &str) -> T {
        self.unwrap_or_else(|err| {
            eprintln!("Fatal internal error: {msg} - {:?}", err);
            restart_app(1);
        })
    }
}

pub struct TrayIcon {}

impl TrayIcon {
    pub fn new() -> tray_icon::TrayIcon {
        let icon = load_tray_icon("#95A5A6");

        let tray_menu = Menu::new();
        let quit_item = MenuItem::new("Quit", true, None);
        let quit_id = quit_item.id().0.clone();
        let restart_item = MenuItem::new("Restart", true, None);
        let restart_id = restart_item.id().0.clone();
        tray_menu.append(&quit_item).unwrap();
        tray_menu.append(&restart_item).unwrap();

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Blade ControlHub")
            .with_icon(icon)
            .build()
            .unwrap();

        MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id {
            id if id == quit_id => {
                tray_app().send(AppEvent::Quit);
            }
            id if id == restart_id => {
                tray_app().send(AppEvent::Restart);
            }
            _ => {}
        }));
        detect_tray_activity_thread();
        tray_icon
    }
}

pub fn set_perf_mode_icon(tray_icon: &mut tray_icon::TrayIcon, perf_mode: PerfMode) {
    println!("Switching tay icon to {} colour", perf_mode);
    let hex = match perf_mode {
        PerfMode::Silent => "#00C853", // Silent
        PerfMode::Quiet => "#00E5FF",
        PerfMode::Balanced => "#FFD600",
        PerfMode::Performance => "#FF5D00",
        PerfMode::Turbo => "#D50000",
        PerfMode::Custom => "#A200FF",
        PerfMode::Unknown => "#95A5A6",
    };

    let new_icon = load_tray_icon(hex);
    tray_icon
        .set_icon(Some(new_icon))
        .expect("Failed to update icon");
}

fn load_tray_icon(hex_color: &str) -> Icon {
    let width = 64;
    let height = 64;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();

    let coloured_svg = include_str!("../../assets/icon.svg")
        .replace("#FFFFFF", hex_color)
        .replace("#ffffff", &hex_color.to_lowercase());

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(&coloured_svg, &opt).expect("Failed to parse SVG");

    let svg_size = tree.size();

    // Sscale it up by an extra 20-30%
    let base_scale = (width as f32 / svg_size.width()).min(height as f32 / svg_size.height());
    let final_scale = base_scale * 1.2;

    // Recalculate centering for the overscaled icon
    let tx = (width as f32 - (svg_size.width() * final_scale)) / 2.0;
    let ty = (height as f32 - (svg_size.height() * final_scale)) / 2.0;

    let transform =
        tiny_skia::Transform::from_scale(final_scale, final_scale).post_translate(tx, ty);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let rgba = pixmap.take();
    Icon::from_rgba(rgba, width, height).expect("Failed to create tray icon")
}

fn detect_tray_activity_thread() {
    thread::spawn(move || {
        loop {
            while let Ok(event) = TrayIconEvent::receiver().recv() {
                if matches!(event, TrayIconEvent::Click { .. }) {
                    device().get_perf_mode();
                }
            }
        }
    });
}
