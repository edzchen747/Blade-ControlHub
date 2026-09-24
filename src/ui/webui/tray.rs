//! Tray icon and menu.
//!
//! The icon is the app glyph recoloured to the active performance mode, so the
//! tray doubles as a persistent readout of what the machine is doing. Menu and
//! click handlers run on Tauri's event loop, so anything that touches the HID
//! device is handed to a worker thread rather than blocking the tray.

use std::sync::Mutex;

use tauri::AppHandle;
use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tracing::{debug, warn};

use crate::razer::enums::PerfMode;
use crate::ui::app::app;
use crate::ui::app_events::AppEvent;
use crate::ui::theme::{
    APP_TOOLTIP, DEFAULT_ICON_COLOR, TRAY_ICON_SCALE_FACTOR, TRAY_ICON_SIZE, perf_mode_hex_color,
};
use crate::win::system::cli_utils::cycle_gpu;

const TRAY_ID: &str = "blade-controlhub-tray";

/// The last performance mode published, kept so a mode reported before the
/// tray exists is still applied once it does. Startup reports a mode well
/// before Tauri's setup hook runs, so without this the icon would sit on the
/// neutral colour until the next change.
static PENDING_PERF_MODE: Mutex<Option<PerfMode>> = Mutex::new(None);

fn pending_perf_mode() -> std::sync::MutexGuard<'static, Option<PerfMode>> {
    PENDING_PERF_MODE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn create(handle: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(handle)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip(APP_TOOLTIP)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(handle_tray_event);

    if let Some(icon) = render_icon(DEFAULT_ICON_COLOR) {
        builder = builder.icon(icon);
    } else {
        warn!("Tray icon could not be rendered; falling back to the executable icon");
    }

    builder.build(handle)?;

    if let Some(pending) = pending_perf_mode().take() {
        apply_perf_mode(handle, pending);
    }

    Ok(())
}

/// Recolours the tray icon for `mode`. Safe to call before the tray exists.
pub fn set_perf_mode_icon(mode: PerfMode) {
    let Some(handle) = super::app_handle() else {
        *pending_perf_mode() = Some(mode);
        return;
    };
    apply_perf_mode(handle, mode);
}

fn apply_perf_mode(handle: &AppHandle, mode: PerfMode) {
    let Some(tray) = handle.tray_by_id(TRAY_ID) else {
        *pending_perf_mode() = Some(mode);
        return;
    };

    debug!(?mode, "Switching tray icon colour");
    if let Some(icon) = render_icon(perf_mode_hex_color(mode)) {
        let _ = tray.set_icon(Some(icon));
    }
    let _ = tray.set_tooltip(Some(format!("{APP_TOOLTIP} - {mode}")));
}

fn build_menu(handle: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(handle)?;
    menu.append(&MenuItem::with_id(
        handle,
        "close_gpu_apps",
        "Close apps running on dGPU",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        handle,
        "settings_window",
        "Settings",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        handle,
        "restart",
        "Restart",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        handle,
        "quit",
        "Quit",
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

fn handle_menu_event(_handle: &AppHandle, event: MenuEvent) {
    match event.id.0.as_str() {
        "quit" => in_background("tray-quit", || app(AppEvent::Shutdown)),
        "restart" => in_background("tray-restart", || app(AppEvent::Restart(0))),
        "settings_window" => super::window::show_window(),
        "close_gpu_apps" => in_background("tray-close-gpu-apps", cycle_gpu),
        other => debug!(id = other, "Ignoring unknown tray menu item"),
    }
}

fn handle_tray_event(tray: &tauri::tray::TrayIcon, event: TrayIconEvent) {
    let _ = tray;
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        debug!("Tray icon clicked, toggling the settings window");
        super::window::toggle_window();
    }
}

/// Runs `work` off the event loop. Tray handlers must return promptly: the
/// same thread pumps the window, and device work can block for seconds.
fn in_background(name: &str, work: impl FnOnce() + Send + 'static) {
    if let Err(error) = std::thread::Builder::new()
        .name(format!("blade-{name}"))
        .spawn(work)
    {
        warn!(%error, name, "Failed to spawn a tray worker thread");
    }
}

fn render_icon(hex_color: &str) -> Option<tauri::image::Image<'static>> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(TRAY_ICON_SIZE, TRAY_ICON_SIZE)?;

    let svg = include_str!("../../../assets/icon.svg")
        .replace("#FFFFFF", hex_color)
        .replace("#ffffff", &hex_color.to_lowercase());

    let tree = match resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()) {
        Ok(tree) => tree,
        Err(error) => {
            warn!(?error, "Failed to parse the tray icon SVG");
            return None;
        }
    };

    let svg_size = tree.size();
    let base_scale =
        (TRAY_ICON_SIZE as f32 / svg_size.width()).min(TRAY_ICON_SIZE as f32 / svg_size.height());
    let scale = base_scale * TRAY_ICON_SCALE_FACTOR;
    let tx = (TRAY_ICON_SIZE as f32 - svg_size.width() * scale) / 2.0;
    let ty = (TRAY_ICON_SIZE as f32 - svg_size.height() * scale) / 2.0;

    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty),
        &mut pixmap.as_mut(),
    );

    Some(tauri::image::Image::new_owned(
        pixmap.take(),
        TRAY_ICON_SIZE,
        TRAY_ICON_SIZE,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_icon_renders_at_the_configured_size() {
        let icon = render_icon("#FFD700").expect("the bundled tray SVG must render");

        assert_eq!(icon.width(), TRAY_ICON_SIZE);
        assert_eq!(icon.height(), TRAY_ICON_SIZE);
    }

    #[test]
    fn a_mode_reported_before_the_tray_exists_is_held_for_it() {
        *pending_perf_mode() = None;

        set_perf_mode_icon(PerfMode::Turbo);

        // No Tauri app is running under test, so the mode must have been kept
        // rather than dropped.
        assert_eq!(pending_perf_mode().take(), Some(PerfMode::Turbo));
    }
}
