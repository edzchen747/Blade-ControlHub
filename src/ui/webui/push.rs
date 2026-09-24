//! Pushes runtime state to the settings window.
//!
//! The window never polls. Anything that can change device state — a hotkey, a
//! power transition, a command from the window itself — calls
//! [`notify_settings_changed`], and a single worker coalesces those into at
//! most one snapshot per debounce window. Reading a snapshot goes through the
//! device queue, so coalescing is what keeps a held hotkey from flooding it.

use std::sync::OnceLock;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tracing::{debug, warn};

use super::state::UiState;
use super::window::WINDOW_LABEL;

/// Event name the window listens on for a full state snapshot.
pub const STATE_EVENT: &str = "state";

/// How long to keep absorbing change notifications before reading the device.
const COALESCE_WINDOW: Duration = Duration::from_millis(60);

static PUSH_TX: OnceLock<Sender<Request>> = OnceLock::new();

enum Request {
    /// Something may have changed; send a snapshot unless one is already due.
    Changed,
    /// The window just became visible; send a snapshot even though nothing
    /// changed, because it may have missed updates while hidden.
    Immediate,
}

pub fn start(handle: AppHandle) {
    let (tx, rx) = mpsc::channel();
    if PUSH_TX.set(tx).is_err() {
        warn!("State push worker was already started");
        return;
    }

    if let Err(error) = std::thread::Builder::new()
        .name("blade-state-push".to_string())
        .spawn(move || run(handle, rx))
    {
        warn!(%error, "Failed to start the state push worker");
    }
}

/// Signals that runtime state may have changed. Cheap and non-blocking: safe
/// to call from hotkey and monitor threads.
pub fn notify_settings_changed() {
    send(Request::Changed);
}

/// Forces a snapshot on the next worker tick, used when the window opens.
pub fn push_now() {
    send(Request::Immediate);
}

fn send(request: Request) {
    if let Some(tx) = PUSH_TX.get() {
        let _ = tx.send(request);
    }
}

fn run(handle: AppHandle, rx: mpsc::Receiver<Request>) {
    while let Ok(first) = rx.recv() {
        let forced = matches!(first, Request::Immediate);
        let forced = drain(&rx, forced);

        // Reading state costs a device round-trip, so skip it entirely while
        // nothing is watching. A newly shown window forces a read instead.
        if !forced && !window_is_visible(&handle) {
            continue;
        }

        emit_snapshot(&handle);
    }
}

/// Absorbs further notifications for the coalesce window, returning whether an
/// immediate push was requested during it.
fn drain(rx: &mpsc::Receiver<Request>, mut forced: bool) -> bool {
    let deadline = std::time::Instant::now() + COALESCE_WINDOW;
    loop {
        let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
            return forced;
        };
        match rx.recv_timeout(remaining) {
            Ok(Request::Immediate) => forced = true,
            Ok(Request::Changed) => {}
            Err(RecvTimeoutError::Timeout) => return forced,
            Err(RecvTimeoutError::Disconnected) => return forced,
        }
    }
}

fn emit_snapshot(handle: &AppHandle) {
    let state = match crate::razer::device_handle::device().get_settings_state() {
        Ok(state) => state,
        Err(error) => {
            warn!(%error, "Failed to read runtime state for the settings window");
            return;
        }
    };

    super::window::apply_accent_icon(state.theme_color);

    if let Err(error) = handle.emit(STATE_EVENT, UiState::new(state)) {
        warn!(%error, "Failed to push runtime state to the settings window");
    } else {
        debug!("Pushed a runtime state snapshot to the settings window");
    }
}

fn window_is_visible(handle: &AppHandle) -> bool {
    handle
        .get_webview_window(WINDOW_LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}
