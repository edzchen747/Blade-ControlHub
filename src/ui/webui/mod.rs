//! The Tauri settings window, tray icon and their command surface.
//!
//! This module owns the process's main thread once `run` is called: Tauri's
//! event loop drives the tray and the WebView2 window. Everything the window
//! can change goes through [`crate::razer::device_handle::DeviceHandle`], the
//! same serialized queue the hotkeys and Windows monitors use, so the window is
//! never a second owner of the HID device.
//!
//! The OSD is deliberately not part of this module. It keeps its own Win32
//! thread and message pump so overlay latency never depends on the webview.

mod command_lab;
mod commands;
mod key_capture;
mod push;
mod state;
mod tray;
mod window;

use std::sync::OnceLock;

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tracing::{info, warn};

pub use command_lab::{
    CommandLabRecordingState, CommandLabStatus, cancel_command_lab_record,
    stop_command_lab_recording,
};
pub use key_capture::{push_fn_state, record_razer_key_code, stop_razer_key_capture};
pub use push::notify_settings_changed;
pub use window::{WINDOW_LABEL, hide_window, show_window, toggle_window};

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// The running Tauri app, once its `setup` hook has completed. Callers outside
/// the event loop use this to reach the tray and window; before startup
/// finishes it is `None` and the caller is expected to skip the update rather
/// than block the hardware path waiting for a UI that may never appear.
pub fn app_handle() -> Option<&'static AppHandle> {
    APP_HANDLE.get()
}

/// Builds and runs the Tauri event loop. Blocks until the app exits, so this
/// is the last call the main thread makes.
pub fn run() {
    let context = tauri::generate_context!();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(commands::handler())
        .setup(|app| {
            let handle = app.handle().clone();
            let _ = APP_HANDLE.set(handle.clone());

            tray::create(&handle)?;
            push::start(handle.clone());

            if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
                window::prepare(&window);
            } else {
                warn!(
                    label = WINDOW_LABEL,
                    "Settings window is missing from the Tauri config"
                );
            }

            info!("Tauri runtime ready");
            Ok(())
        })
        .build(context);

    let app = match app {
        Ok(app) => app,
        Err(error) => {
            warn!(%error, "Failed to build the Tauri runtime; the tray and settings window are unavailable");
            return;
        }
    };

    app.run(|_handle, event| match event {
        // The window is a tray app surface, not the app's lifetime: closing it
        // must leave the hardware runtime, hotkeys and OSD running.
        RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::Focused(focused),
            ..
        } if label == WINDOW_LABEL => window::report_focus(focused),
        _ => {}
    });
}

/// Ends the Tauri event loop, which returns control to `main`.
pub fn request_exit() {
    if let Some(handle) = app_handle() {
        handle.exit(0);
    }
}

pub use state::UiState;
pub use tray::set_perf_mode_icon;
