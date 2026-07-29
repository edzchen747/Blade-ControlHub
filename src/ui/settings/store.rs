/// A thread-safe, cloneable handle to the Settings state.
///
/// This type encapsulates `Arc<Mutex<Settings>>` internally, providing a clean
/// API that hides the lock/unlock pattern from callers. This is necessary because
/// egui's `show_viewport_deferred` spawns a secondary viewport whose closure
/// must own its captured data.
use std::sync::{Arc, Mutex, MutexGuard};

use super::Settings;
use crate::runtime::settings_state::SettingsState;

#[derive(Clone)]
pub struct SettingsStore {
    inner: Arc<Mutex<Settings>>,
}

impl SettingsStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Settings::new())),
        }
    }

    /// Shows the settings window with the given config.
    pub fn show(&self, state: SettingsState) {
        self.settings().show(state);
    }

    pub fn update_state(&self, state: SettingsState) {
        self.settings().update_state(state);
    }

    /// Toggles the settings window visibility.
    pub fn toggle(&self, state: SettingsState) {
        self.settings().toggle(state);
    }

    /// Returns whether the settings window is currently visible.
    #[allow(dead_code)]
    pub fn is_visible(&self) -> bool {
        self.settings().show
    }

    /// Requests that the settings window come to the front on next frame.
    #[allow(dead_code)]
    pub fn request_focus(&self) {
        let mut settings = self.settings();
        settings.update = true;
    }

    /// Hides the settings window.
    #[allow(dead_code)]
    pub fn hide(&self) {
        self.settings().show = false;
    }

    /// Sets a captured Razer key code in the currently listening slot.
    pub fn set_razer_key_code(&self, key_code: u8) {
        self.settings().custom_key_map.special_key = Some(key_code);
    }

    /// Runs a short mutation against the settings state with poison recovery.
    pub fn with_settings<R>(&self, f: impl FnOnce(&mut Settings) -> R) -> R {
        let mut settings = self.settings();
        f(&mut settings)
    }

    fn settings(&self) -> MutexGuard<'_, Settings> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new()
    }
}
