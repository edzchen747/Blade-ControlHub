use crate::razer::device_handle::device;
use crate::win::audio::{self, AudioType};
use crate::win::input::blocker::KeyBlocker;
use crate::win::input::hidapi::HidApiListener;
use crate::win::input::key;
use crate::win::input::trackpad::toggle_trackpad;

use once_cell::sync::Lazy;
use rdev::{EventType, Key, simulate};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

pub static FN_PRESSED: AtomicBool = AtomicBool::new(false);
pub static ALT_PRESSED: AtomicBool = AtomicBool::new(false);
pub static DEFAULT_MULTIMEDIA_KEYS: AtomicBool = AtomicBool::new(true);

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
        for key in self {
            let _ = simulate(&EventType::KeyPress(*key));
            thread::sleep(Duration::from_millis(20));
        }
        for key in self.into_iter().rev() {
            let _ = simulate(&EventType::KeyRelease(*key));
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl<'a> IntoIterator for &'a KeyCombo {
    type Item = &'a Key;
    type IntoIter = std::iter::Flatten<std::slice::Iter<'a, Option<Key>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().flatten()
    }
}

pub enum Source<'a> {
    IsTrue(&'a AtomicBool),
    IsFalse(&'a AtomicBool),
}

impl<'a> Source<'a> {
    fn eval(&self) -> bool {
        match self {
            Source::IsTrue(atomic) => atomic.load(Ordering::SeqCst),
            Source::IsFalse(atomic) => !atomic.load(Ordering::SeqCst),
        }
    }
}

pub struct KeyEventAction<'a> {
    action: Box<dyn Fn() + Send + Sync>,
    condition: Vec<Source<'a>>,
}

impl<'a> KeyEventAction<'a> {
    pub fn new(action: Box<dyn Fn() + Send + Sync>, condition: Vec<Source<'a>>) -> Self {
        Self { action, condition }
    }
    pub fn execute(&self) -> bool {
        let conditions_met = self.condition.iter().all(|check| check.eval());
        if conditions_met {
            (self.action)();
        }
        conditions_met
    }
}

pub static KEY_MAP: Lazy<HashMap<key::Key, KeyEventAction>> = Lazy::new(|| {
    HashMap::from([
        (
            key::Key::B,
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_battery_limit();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            key::Key::P,
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_perf_mode();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            key::Key::R,
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_refresh_rate();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            key::Key::T,
            KeyEventAction::new(
                Box::new(|| {
                    toggle_trackpad();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            key::Key::V,
            KeyEventAction::new(
                Box::new(|| {
                    device().toggle_vc();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            key::Key::F1,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(173)]).trigger();
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::F2,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(174)]).trigger();
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::F3,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(175)]).trigger();
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::F4,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::MetaLeft, Key::KeyP]).trigger();
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::F5,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(177)]).trigger();
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::F6,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(179)]).trigger();
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::F7,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(176)]).trigger();
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::F8,
            KeyEventAction::new(
                Box::new(|| {
                    device().adjust_screen_brightness(-10);
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::F9,
            KeyEventAction::new(
                Box::new(|| {
                    device().adjust_screen_brightness(10);
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::F10,
            KeyEventAction::new(
                Box::new(|| {
                    device().keyboard_light_down();
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::F11,
            KeyEventAction::new(
                Box::new(|| {
                    device().keyboard_light_up();
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::F12,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::PrintScreen]).trigger();
                }),
                vec![
                    Source::IsTrue(&DEFAULT_MULTIMEDIA_KEYS),
                    Source::IsFalse(&FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            key::Key::Mic,
            KeyEventAction::new(
                Box::new(|| {
                    audio::toggle_audio_mute(AudioType::Mic);
                }),
                vec![],
            ),
        ),
        (
            key::Key::Trackpad,
            KeyEventAction::new(
                Box::new(|| {
                    toggle_trackpad();
                }),
                vec![],
            ),
        ),
        (
            key::Key::Perf,
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_perf_mode();
                }),
                vec![],
            ),
        ),
        (
            key::Key::CoPilot,
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_rgb_mode();
                }),
                vec![],
            ),
        ),
        (
            key::Key::Home,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(36)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            key::Key::Up,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(38)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            key::Key::PgUp,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(33)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            key::Key::Left,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(37)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            key::Key::Right,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(39)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            key::Key::End,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(35)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            key::Key::Down,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(40)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            key::Key::PgDn,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(34)]).trigger();
                }),
                vec![],
            ),
        ),
    ])
});
