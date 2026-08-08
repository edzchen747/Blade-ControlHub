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

    pub fn show(&self, state: SettingsState) {
        self.settings().show(state);
    }

    pub fn update_state(&self, state: SettingsState) {
        self.settings().update_state(state);
    }

    pub fn toggle(&self, state: SettingsState) {
        self.settings().toggle(state);
    }

    #[allow(dead_code)]
    pub fn is_visible(&self) -> bool {
        self.settings().show
    }

    #[allow(dead_code)]
    pub fn request_focus(&self) {
        let mut settings = self.settings();
        settings.update = true;
    }

    #[allow(dead_code)]
    pub fn hide(&self) {
        self.settings().show = false;
    }

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
