use super::tray_icon_module::load_tray_icon;
/// Tray icon orchestrator.
///
/// This module manages the system tray lifecycle by delegating icon
/// rasterization to `tray_icon` and menu construction to `tray_menu`.
use super::tray_menu as tray_menu_module;

pub use super::tray_icon_module::set_perf_mode_icon;
pub use tray_menu_module::{build_tray_menu, setup_menu_event_handler, spawn_tray_click_listener};

use crate::ui::theme::{APP_TOOLTIP, DEFAULT_ICON_COLOR};

/// Builds and initializes the system tray icon with its context menu.
pub fn build_tray_icon() -> tray_icon::TrayIcon {
    let icon = load_tray_icon(DEFAULT_ICON_COLOR);
    let tray_menu = build_tray_menu();

    let tray_icon = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip(APP_TOOLTIP)
        .with_icon(icon)
        .build()
        .unwrap();

    setup_menu_event_handler();
    spawn_tray_click_listener();

    tray_icon
}
