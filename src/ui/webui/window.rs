//! Settings window lifecycle.
//!
//! The window is created hidden at startup and reused for the lifetime of the
//! process. Closing it hides it: the webview stays warm, so the tray click that
//! reopens it is instant, and — more importantly — closing a window must never
//! take down the hardware runtime that now lives in the same process.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Manager, PhysicalPosition, WebviewWindow, WindowEvent};
use tracing::{debug, warn};

use crate::config::ThemeColor;
use crate::ui::theme::{SETTINGS_ICON_SIZE, SETTINGS_PADDING_RATIO};

pub const WINDOW_LABEL: &str = "main";

/// Tracks the visibility the runtime has been told about, so focus changes
/// arriving after a hide do not re-arm OSD suppression.
static WINDOW_OPEN: AtomicBool = AtomicBool::new(false);

/// Whether the window has been positioned since the process started. The
/// bottom-right placement is applied once, then the user's own moves stick.
static PLACED: AtomicBool = AtomicBool::new(false);

pub fn prepare(window: &WebviewWindow) {
    let handle = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            hide(&handle);
        }
    });
}

pub fn show_window() {
    let Some(window) = window() else {
        return;
    };
    place_once(&window);

    if let Err(error) = window.show() {
        warn!(%error, "Failed to show the settings window");
        return;
    }
    if let Err(error) = window.unminimize() {
        debug!(%error, "Settings window could not be unminimised");
    }
    if let Err(error) = window.set_focus() {
        warn!(%error, "Failed to focus the settings window");
    }

    WINDOW_OPEN.store(true, Ordering::SeqCst);
    crate::ui::app::set_settings_window_state(true, true);
    super::push::push_now();
}

pub fn hide_window() {
    if let Some(window) = window() {
        hide(&window);
    }
}

pub fn toggle_window() {
    let Some(window) = window() else {
        return;
    };

    if window.is_visible().unwrap_or(false) {
        hide(&window);
    } else {
        show_window();
    }
}

/// Reports webview focus to the runtime, which suppresses the OSD while the
/// user is adjusting the same settings in the window.
pub fn report_focus(focused: bool) {
    if !WINDOW_OPEN.load(Ordering::SeqCst) {
        return;
    }
    crate::ui::app::set_settings_window_state(true, focused);
    if focused {
        super::push::push_now();
    }
}

/// Repaints the taskbar and title-bar icon in the accent colour.
pub fn apply_accent_icon(color: ThemeColor) {
    let Some(window) = window() else {
        return;
    };
    let Some(icon) = accent_icon(color) else {
        return;
    };
    if let Err(error) = window.set_icon(icon) {
        warn!(%error, "Failed to apply the accent-coloured window icon");
    }
}

fn hide(window: &WebviewWindow) {
    if let Err(error) = window.hide() {
        warn!(%error, "Failed to hide the settings window");
        return;
    }
    WINDOW_OPEN.store(false, Ordering::SeqCst);
    crate::ui::app::set_settings_window_state(false, false);
}

fn window() -> Option<WebviewWindow> {
    let handle = super::app_handle()?;
    match handle.get_webview_window(WINDOW_LABEL) {
        Some(window) => Some(window),
        None => {
            warn!(label = WINDOW_LABEL, "Settings window is not available");
            None
        }
    }
}

/// Places the window above the tray on first show. Later shows leave it where
/// the user put it.
fn place_once(window: &WebviewWindow) {
    if PLACED.swap(true, Ordering::SeqCst) {
        return;
    }

    let Ok(Some(monitor)) = window.primary_monitor() else {
        debug!("No primary monitor reported; leaving the settings window where Windows placed it");
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };

    let screen = monitor.size();
    let position = spawn_position(
        (screen.width, screen.height),
        (size.width, size.height),
        monitor.scale_factor(),
    );

    if let Err(error) = window.set_position(PhysicalPosition::new(position.0, position.1)) {
        warn!(%error, "Failed to place the settings window above the tray");
    }
}

/// Bottom-right of the primary monitor, clear of the taskbar. Returns physical
/// pixels relative to the monitor origin.
fn spawn_position(screen: (u32, u32), window: (u32, u32), scale: f64) -> (i32, i32) {
    let _ = scale;
    let padding = (screen.1 as f32 * SETTINGS_PADDING_RATIO) as i32;
    let x = screen.0 as i32 - window.0 as i32 - padding / 10;
    let y = screen.1 as i32 - window.1 as i32 - padding;
    (x.max(0), y.max(0))
}

fn accent_icon(color: ThemeColor) -> Option<tauri::image::Image<'static>> {
    let size = SETTINGS_ICON_SIZE;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;

    let hex = color.to_hex_string();
    let svg = include_str!("../../../assets/settings_icon.svg")
        .replace("#FFFFFF", &hex)
        .replace("#ffffff", &hex.to_lowercase());

    let tree = match resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()) {
        Ok(tree) => tree,
        Err(error) => {
            warn!(?error, "Failed to parse the settings window icon SVG");
            return None;
        }
    };

    let svg_size = tree.size();
    let scale = (size as f32 / svg_size.width()).min(size as f32 / svg_size.height());
    let tx = (size as f32 - svg_size.width() * scale) / 2.0;
    let ty = (size as f32 - svg_size.height() * scale) / 2.0;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty),
        &mut pixmap.as_mut(),
    );

    Some(tauri::image::Image::new_owned(pixmap.take(), size, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_position_sits_above_the_tray_in_the_bottom_right() {
        let (x, y) = spawn_position((1920, 1080), (880, 620), 1.0);

        assert_eq!(x, 1920 - 880 - 10);
        assert_eq!(y, 1080 - 620 - 108);
    }

    #[test]
    fn spawn_position_never_pushes_the_window_off_screen() {
        let (x, y) = spawn_position((800, 600), (1200, 900), 1.0);

        assert_eq!((x, y), (0, 0));
    }
}
