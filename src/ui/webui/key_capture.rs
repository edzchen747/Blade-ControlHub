//! Capture of Razer special keys for the Keys page.
//!
//! These keys never reach Windows as virtual key codes — they arrive as HID
//! reports on the device's own interface — so the window cannot listen for
//! them itself. While a capture is armed the HID reader forwards the code here
//! instead of running its mapped action, and the code is pushed straight to the
//! window.

use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::Emitter;
use tracing::{debug, warn};

use crate::core::shared_state::KEYMAP_LISTENING;

/// Event name the window listens on for a captured Razer key.
pub const RAZER_KEY_EVENT: &str = "razer-key";

#[derive(Clone, Copy, Debug, Serialize)]
pub struct CapturedRazerKey {
    pub key_code: u8,
}

/// Arms capture. The next Razer special key is delivered to the window as a
/// [`RAZER_KEY_EVENT`] instead of triggering its mapped action.
pub fn begin_razer_key_capture() {
    KEYMAP_LISTENING.store(true, Ordering::SeqCst);
    debug!("Armed Razer special key capture");
}

/// Disarms capture, restoring normal key handling.
pub fn stop_razer_key_capture() {
    KEYMAP_LISTENING.store(false, Ordering::SeqCst);
}

/// Called by the HID reader for every Razer special key. Consumes the key only
/// while a capture is armed; otherwise it does nothing and the reader runs the
/// mapped action as usual.
pub fn record_razer_key_code(key_code: u8) {
    if !KEYMAP_LISTENING.load(Ordering::SeqCst) {
        return;
    }
    stop_razer_key_capture();

    let Some(handle) = super::app_handle() else {
        warn!(key_code, "Captured a Razer key before the window was ready");
        return;
    };

    if let Err(error) = handle.emit(RAZER_KEY_EVENT, CapturedRazerKey { key_code }) {
        warn!(%error, key_code, "Failed to deliver a captured Razer key to the window");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_arms_and_disarms_the_shared_listening_flag() {
        begin_razer_key_capture();
        assert!(KEYMAP_LISTENING.load(Ordering::SeqCst));

        stop_razer_key_capture();
        assert!(!KEYMAP_LISTENING.load(Ordering::SeqCst));
    }

    #[test]
    fn recording_a_key_without_an_armed_capture_leaves_the_flag_clear() {
        stop_razer_key_capture();

        record_razer_key_code(0x42);

        assert!(!KEYMAP_LISTENING.load(Ordering::SeqCst));
    }
}
