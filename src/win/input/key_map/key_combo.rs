use std::sync::atomic::Ordering;

use crate::core::shared_state::{ALT_PRESSED, DEFAULT_MULTIMEDIA_KEYS, FN_PRESSED};
use crate::razer::device_handle::device;
use crate::win::audio::{self, AudioType};
use crate::win::input::trackpad::toggle_trackpad;
use crate::win::input::{KeyType, razer_key, vkey};

use once_cell::sync::Lazy;
use rdev::{EventType, Key, simulate};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::Duration;
use tracing::warn;

pub struct KeyCombo([Option<Key>; 4]);

impl KeyCombo {
    pub fn new(input: &[Key]) -> Self {
        let mut buffer = [None; 4];
        for (i, key) in input.iter().take(4).enumerate() {
            buffer[i] = Some(*key);
        }
        Self(buffer)
    }

    pub fn trigger(&self) {
        let failed_events = self.try_trigger();
        if failed_events > 0 {
            warn!(failed_events, "One or more simulated key events failed");
        }
    }

    fn try_trigger(&self) -> usize {
        let mut failed_events = 0;
        for event in self.events() {
            if let Err(error) = simulate(&event) {
                failed_events += 1;
                warn!(?event, ?error, "Failed to simulate key event");
            }
            thread::sleep(Duration::from_millis(10));
        }
        failed_events
    }

    fn events(&self) -> Vec<EventType> {
        self.into_iter()
            .copied()
            .map(EventType::KeyPress)
            .chain(self.into_iter().rev().copied().map(EventType::KeyRelease))
            .collect()
    }
}

impl<'a> IntoIterator for &'a KeyCombo {
    type Item = &'a Key;
    type IntoIter = std::iter::Flatten<std::slice::Iter<'a, Option<Key>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().flatten()
    }
}

