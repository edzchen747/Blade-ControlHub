use std::sync::atomic::{AtomicBool, Ordering};

/// Data structures for custom key mapping state.
///
/// Extracted from `Settings` to provide a clean, testable domain model
/// that can be shared across UI components without coupling to egui.

#[derive(Default, Clone)]
#[allow(dead_code)]
pub struct FuncKeyMap {
    pub key: String,
    pub action: String,
}

#[derive(Default, Clone)]
pub struct RazerKeyMap {
    pub key_code: u8,
    pub name: String,
    pub action: String,
}

pub static KEYMAP_LISTENING: AtomicBool = AtomicBool::new(false);

/// The custom key mapping state, holding both function key and Razer special key rows.
#[derive(Default)]
pub struct CustomKeyMap {
    #[allow(dead_code)]
    pub func_keys: Vec<FuncKeyMap>,
    pub razer_keys: Vec<RazerKeyMap>,
    listening_idx: Option<usize>,
    pub special_key: Option<u8>,
}

impl CustomKeyMap {
    pub fn new() -> Self {
        Self {
            func_keys: vec![FuncKeyMap::default()],
            razer_keys: vec![RazerKeyMap::default()],
            listening_idx: None,
            special_key: None,
        }
    }

    /// Resets a key code in all rows if it's already assigned (prevent duplicates).
    pub fn reset_key_code(&mut self, key_code: u8) {
        for row in self.razer_keys.iter_mut() {
            if row.key_code == key_code {
                row.key_code = 0;
            }
        }
    }

    pub fn set_listening_idx(&mut self, idx: Option<usize>) {
        self.listening_idx = idx;
        KEYMAP_LISTENING.store(idx.is_some(), Ordering::SeqCst);
    }

    pub fn get_listening_idx(&self) -> Option<usize> {
        self.listening_idx
    }
}
