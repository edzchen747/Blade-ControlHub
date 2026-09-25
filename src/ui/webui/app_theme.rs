//! Keeps the settings window in the Windows app mode as it changes.
//!
//! The page draws its own light and dark palettes, so it is told the mode
//! directly with [`APP_THEME_EVENT`] rather than left to WebView2's
//! `prefers-color-scheme`, which did not follow a change made while the app was
//! running. The native frame is set to match, so the title bar and the page
//! never disagree.

use tauri::{Emitter, Manager, Theme};
use tracing::{debug, warn};

use crate::win::system::app_theme::{self, AppTheme};

use super::window::WINDOW_LABEL;

/// Event name the window listens on for the app mode, `"light"` or `"dark"`.
pub const APP_THEME_EVENT: &str = "app-theme";

/// Brings the window frame into line with the current mode, then follows it.
pub fn start() {
    apply_to_frame(app_theme::current());
    app_theme::watch(|theme| {
        apply_to_frame(theme);
        push(theme);
    });
}

/// The mode as Windows has it now, for the page's first paint.
pub fn current() -> AppTheme {
    app_theme::current()
}

fn push(theme: AppTheme) {
    let Some(handle) = super::app_handle() else {
        return;
    };
    if let Err(error) = handle.emit(APP_THEME_EVENT, theme) {
        debug!(%error, ?theme, "Failed to push the app mode to the settings window");
    }
}

fn apply_to_frame(theme: AppTheme) {
    let Some(window) = super::app_handle().and_then(|handle| handle.get_webview_window(WINDOW_LABEL))
    else {
        return;
    };
    let theme = match theme {
        AppTheme::Light => Theme::Light,
        AppTheme::Dark => Theme::Dark,
    };
    if let Err(error) = window.set_theme(Some(theme)) {
        warn!(%error, ?theme, "Failed to apply the app mode to the settings window frame");
    }
}
