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

/// Event name the window listens on for Fn being held or released.
pub const FN_STATE_EVENT: &str = "fn-state";

/// Tells the window whether Fn is held.
///
/// Fn never reaches Windows as a virtual key, so the window cannot observe it;
/// it needs this to decide whether a key press of its own is a Hypershift
/// press that the runtime will consume.
pub fn push_fn_state(pressed: bool) {
    let Some(handle) = super::app_handle() else {
        return;
    };
    if let Err(error) = handle.emit(FN_STATE_EVENT, pressed) {
        debug!(%error, pressed, "Failed to push Fn state to the settings window");
    }
}

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

/// Called by the HID reader for every Razer special key. Returns whether the
/// key was consumed by a capture: disarming happens here, so the reader cannot
/// tell by re-reading the flag, and a key being bound must not also run the
/// action it is being bound away from.
pub fn record_razer_key_code(key_code: u8) -> bool {
    if !KEYMAP_LISTENING.load(Ordering::SeqCst) {
        return false;
    }
    stop_razer_key_capture();

    let Some(handle) = super::app_handle() else {
        warn!(key_code, "Captured a Razer key before the window was ready");
        return true;
    };

    if let Err(error) = handle.emit(RAZER_KEY_EVENT, CapturedRazerKey { key_code }) {
        warn!(%error, key_code, "Failed to deliver a captured Razer key to the window");
    }
    true
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

        assert!(!record_razer_key_code(0x42), "nothing was armed to consume it");
        assert!(!KEYMAP_LISTENING.load(Ordering::SeqCst));
    }

    /// The reader decides whether to run the key's action from this return
    /// value: the flag it would otherwise check has already been cleared here.
    #[test]
    fn an_armed_capture_consumes_the_key() {
        begin_razer_key_capture();

        assert!(record_razer_key_code(0x42));
        assert!(!KEYMAP_LISTENING.load(Ordering::SeqCst));
    }
}
