//! Coalesced cross-process wake-up notifications for the settings window.
//!
//! The settings process still reads state through the runtime IPC server. This
//! event only tells it when to request a fresh snapshot.

use std::sync::OnceLock;
use std::time::Duration;

use tracing::warn;
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        System::Threading::{CreateEventW, SetEvent, WaitForSingleObject},
    },
    core::w,
};

const SETTINGS_UPDATE_EVENT_NAME: windows::core::PCWSTR =
    w!("Local\\BladeControlHubSettingsUpdate");
static RUNTIME_UPDATE_EVENT: OnceLock<Option<usize>> = OnceLock::new();

/// Signals that a successful runtime hardware command may have changed the
/// settings snapshot. The auto-reset event coalesces repeated updates.
pub fn notify_settings_updated() {
    let Some(event) =
        *RUNTIME_UPDATE_EVENT.get_or_init(|| create_update_event().map(|event| event.0 as usize))
    else {
        return;
    };

    // SAFETY: `event` is a valid named event handle held for the process
    // lifetime, and SetEvent is safe to call from concurrent command threads.
    if let Err(error) = unsafe { SetEvent(HANDLE(event as *mut _)) } {
        warn!(%error, "Failed to notify settings window of runtime update");
    }
}

pub struct SettingsUpdateListener {
    event: HANDLE,
}

impl SettingsUpdateListener {
    pub fn new() -> Option<Self> {
        create_update_event().map(|event| Self { event })
    }

    /// Waits until a runtime update is available or the timeout elapses.
    pub fn wait(&self, timeout: Duration) -> bool {
        let timeout_ms = timeout.as_millis().min(u32::MAX as u128) as u32;

        // SAFETY: `self.event` is owned by this listener and remains valid for
        // the listener lifetime.
        unsafe { WaitForSingleObject(self.event, timeout_ms) == WAIT_OBJECT_0 }
    }
}

impl Drop for SettingsUpdateListener {
    fn drop(&mut self) {
        // SAFETY: this listener uniquely owns this event handle.
        let _ = unsafe { CloseHandle(self.event) };
    }
}

fn create_update_event() -> Option<HANDLE> {
    // SAFETY: the event is process-local to the current user session and uses
    // the default security descriptor, matching the existing local IPC scope.
    match unsafe { CreateEventW(None, false, false, SETTINGS_UPDATE_EVENT_NAME) } {
        Ok(event) => Some(event),
        Err(error) => {
            warn!(%error, "Failed to create settings update event");
            None
        }
    }
}
