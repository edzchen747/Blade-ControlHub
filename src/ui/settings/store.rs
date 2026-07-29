/// A thread-safe, cloneable handle to the Settings state.
///
/// This type encapsulates `Arc<Mutex<Settings>>` internally, providing a clean
/// API that hides the lock/unlock pattern from callers. This is necessary because
/// egui's `show_viewport_deferred` spawns a secondary viewport whose closure
/// must own its captured data.
use std::sync::{Arc, Mutex};

use eframe::egui;

use super::Settings;
use crate::razer::config::AppConfig;

#[derive(Clone)]
pub struct SettingsStore {
    pub inner: Arc<Mutex<Settings>>,
}

impl SettingsStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Settings::new())),
        }
    }

    /// Shows the settings window with the given config.
    pub fn show(&self, config: AppConfig) {
        self.inner.lock().unwrap().show(config);
    }

    /// Toggles the settings window visibility.
    pub fn toggle(&self, config: AppConfig) {
        self.inner.lock().unwrap().toggle(config);
    }

    /// Returns whether the settings window is currently visible.
    #[allow(dead_code)]
    pub fn is_visible(&self) -> bool {
        self.inner.lock().unwrap().show
    }

    /// Requests that the settings window come to the front on next frame.
    #[allow(dead_code)]
    pub fn request_focus(&self) {
        let mut settings = self.inner.lock().unwrap();
        settings.update = true;
    }

    /// Hides the settings window.
    #[allow(dead_code)]
    pub fn hide(&self) {
        self.inner.lock().unwrap().show = false;
    }

    /// Sets a captured Razer key code in the currently listening slot.
    pub fn set_razer_key_code(&self, key_code: u8) {
        self.inner.lock().unwrap().custom_key_map.special_key = Some(key_code);
    }
}
