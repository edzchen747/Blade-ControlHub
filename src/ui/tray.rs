use crate::razer::device_handle::device;
use crate::razer::enums::PerfMode;
use crate::ui::app::app;
use crate::ui::app_events::{AppEvent, OsdEvent};
use crate::utils::reload::restart_app;
use crate::win::system::startup::Startup;

use resvg::{tiny_skia, usvg};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use tray_icon::menu::CheckMenuItem;
use tray_icon::{
    Icon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

// ── Constants ───────────────────────────────────────────────────────────────

const TRAY_ICON_SIZE: u32 = 64;
const TRAY_ICON_SCALE_FACTOR: f32 = 1.2;
const DEFAULT_ICON_COLOR: &str = "#95A5A6";
const APP_TOOLTIP: &str = "Blade ControlHub";

// ── Startup State ───────────────────────────────────────────────────────────

static STARTUP_STATE: AtomicBool = AtomicBool::new(false);

// ── Tray Icon Builder ───────────────────────────────────────────────────────

/// Builds and initializes the system tray icon with its context menu.
pub fn build_tray_icon() -> tray_icon::TrayIcon {
    let icon = load_tray_icon(DEFAULT_ICON_COLOR);
    let tray_menu = build_tray_menu();

    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip(APP_TOOLTIP)
        .with_icon(icon)
        .build()
        .unwrap();

    setup_menu_event_handler();
    spawn_tray_click_listener();

    tray_icon
}

// ── Perf Mode Icon ──────────────────────────────────────────────────────────

/// Updates the tray icon color to reflect the current performance mode.
pub fn set_perf_mode_icon(tray_icon: &mut tray_icon::TrayIcon, perf_mode: PerfMode) {
    println!("Switching tray icon to {} colour", perf_mode);
    let hex = perf_mode_color(perf_mode);
    let new_icon = load_tray_icon(hex);
    tray_icon
        .set_icon(Some(new_icon))
        .expect("Failed to update icon");
}

/// Maps a `PerfMode` to its corresponding tray icon hex color.
fn perf_mode_color(mode: PerfMode) -> &'static str {
    match mode {
        PerfMode::Silent => "#00C853",
        PerfMode::Quiet => "#00E5FF",
        PerfMode::Balanced => "#FFD600",
        PerfMode::Performance => "#FF5D00",
        PerfMode::Turbo => "#D50000",
        PerfMode::Custom => "#A200FF",
        PerfMode::Unknown => DEFAULT_ICON_COLOR,
    }
}

// ── SVG Icon Rendering ──────────────────────────────────────────────────────

/// Renders the tray icon SVG with the given hex color and returns an `Icon`.
fn load_tray_icon(hex_color: &str) -> Icon {
    let mut pixmap = tiny_skia::Pixmap::new(TRAY_ICON_SIZE, TRAY_ICON_SIZE).unwrap();

    let coloured_svg = include_str!("../../assets/icon.svg")
        .replace("#FFFFFF", hex_color)
        .replace("#ffffff", &hex_color.to_lowercase());

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(&coloured_svg, &opt).expect("Failed to parse SVG");

    let svg_size = tree.size();
    let base_scale =
        (TRAY_ICON_SIZE as f32 / svg_size.width()).min(TRAY_ICON_SIZE as f32 / svg_size.height());
    let final_scale = base_scale * TRAY_ICON_SCALE_FACTOR;

    let tx = (TRAY_ICON_SIZE as f32 - (svg_size.width() * final_scale)) / 2.0;
    let ty = (TRAY_ICON_SIZE as f32 - (svg_size.height() * final_scale)) / 2.0;

    let transform =
        tiny_skia::Transform::from_scale(final_scale, final_scale).post_translate(tx, ty);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let rgba = pixmap.take();
    Icon::from_rgba(rgba, TRAY_ICON_SIZE, TRAY_ICON_SIZE).expect("Failed to create tray icon")
}

// ── Menu Construction ───────────────────────────────────────────────────────

/// Builds the tray context menu with Quit, Restart, and Startup toggle items.
fn build_tray_menu() -> Menu {
    let tray_menu = Menu::new();

    let quit_item = MenuItem::with_id("quit", "Quit", true, None);
    let restart_item = MenuItem::with_id("restart", "Restart", true, None);
    let settings_item = MenuItem::with_id("settings_window", "Settings", true, None);

    let startup_detected = Startup::is_registered();
    let default_multimedia_keys = device().get_default_multimedia_keys();
    let startup_item = CheckMenuItem::with_id(
        "startup_toggle",
        "Start with Windows",
        true,
        startup_detected,
        None,
    );
    let default_keys_item = CheckMenuItem::with_id(
        "default_multimedia_keys",
        "Default Multimedia Keys",
        true,
        default_multimedia_keys,
        None,
    );
    STARTUP_STATE.store(startup_detected, Ordering::SeqCst);

    tray_menu.append(&quit_item).unwrap();
    tray_menu.append(&restart_item).unwrap();
    tray_menu.append(&settings_item).unwrap();
    tray_menu.append(&startup_item).unwrap();
    tray_menu.append(&default_keys_item).unwrap();

    tray_menu
}

/// Registers the global menu event handler for tray menu actions.
fn setup_menu_event_handler() {
    let startup_state = Arc::new(&STARTUP_STATE);

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id.0.as_str() {
        "quit" => {
            app().send(AppEvent::Shutdown);
        }
        "restart" => {
            restart_app(0);
        }
        "settings_window" => app().send(AppEvent::OpenSettings),
        "startup_toggle" => {
            let is_checked = !startup_state.load(Ordering::SeqCst);
            startup_state.store(is_checked, Ordering::SeqCst);
            if is_checked && !Startup::is_registered() {
                Startup::register();
            } else if !is_checked && Startup::is_registered() {
                Startup::unregister();
            }
        }
        "default_multimedia_keys" => {
            let new_default = device().toggle_default_multimedia_keys();
            app().send(OsdEvent::ToggleDefaultMultimediaKeys(new_default).into())
        }
        _ => {}
    }));
}

// ── Tray Click Listener ─────────────────────────────────────────────────────

/// Spawns a background thread that listens for left-clicks on the tray icon
/// and triggers a performance mode query.
fn spawn_tray_click_listener() {
    thread::spawn(move || {
        loop {
            if let Ok(TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }) = TrayIconEvent::receiver().recv()
            {
                println!("Tray clicked");
                app().send(AppEvent::ToggleSettings);
            }
        }
    });
}
