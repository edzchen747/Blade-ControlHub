/// Data structures for custom key mapping state.
///
/// Extracted from `Settings` to provide a clean, testable domain model
/// that can be shared across UI components without coupling to egui.

#[derive(Default, Clone)]
pub struct HypershiftKeyMap {
    /// Windows virtual-key code for the ordinary keyboard key selected in settings.
    pub key_code: Option<u8>,
    /// Reserved for the action editor, which has not been implemented yet.
    pub action: String,
}

#[derive(Default, Clone)]
pub struct RazerKeyMap {
    pub key_code: u8,
    pub name: String,
    pub action: String,
}

/// The custom key mapping state, holding both Hypershift and Razer special key rows.
#[derive(Default)]
pub struct CustomKeyMap {
    pub hypershift_keys: Vec<HypershiftKeyMap>,
    pub razer_keys: Vec<RazerKeyMap>,
    razer_listening_idx: Option<usize>,
    hypershift_listening_idx: Option<usize>,
    pub special_key: Option<u8>,
}

impl CustomKeyMap {
    pub fn new() -> Self {
        Self {
            hypershift_keys: vec![HypershiftKeyMap::default()],
            razer_keys: vec![RazerKeyMap::default()],
            razer_listening_idx: None,
            hypershift_listening_idx: None,
            special_key: None,
        }
    }

    pub fn razer_key_code_is_assigned_elsewhere(&self, row_idx: usize, key_code: u8) -> bool {
        self.razer_keys
            .iter()
            .enumerate()
            .any(|(idx, row)| idx != row_idx && row.key_code == key_code)
    }

    pub fn set_listening_idx(&mut self, idx: Option<usize>) {
        self.razer_listening_idx = idx;
    }

    pub fn get_listening_idx(&self) -> Option<usize> {
        self.razer_listening_idx
    }

    pub fn hypershift_key_code_is_assigned_elsewhere(&self, row_idx: usize, key_code: u8) -> bool {
        self.hypershift_keys
            .iter()
            .enumerate()
            .any(|(idx, row)| idx != row_idx && row.key_code == Some(key_code))
    }

    pub fn set_hypershift_listening_idx(&mut self, idx: Option<usize>) {
        self.hypershift_listening_idx = idx;
    }

    pub fn hypershift_listening_idx(&self) -> Option<usize> {
        self.hypershift_listening_idx
    }
}
