use crate::razer::device_handle::device;
use crate::win::audio::{self, AudioType};
use crate::win::input::trackpad::toggle_trackpad;
use crate::win::input::{KeyType, razer_key, vkey};

use once_cell::sync::Lazy;
use rdev::{EventType, Key, simulate};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

pub static FN_PRESSED: AtomicBool = AtomicBool::new(false);
pub static ALT_PRESSED: AtomicBool = AtomicBool::new(false);
pub static SHIFT_PRESSED: AtomicBool = AtomicBool::new(false);
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

pub static KEY_MAP: Lazy<HashMap<KeyType, KeyEventAction>> = Lazy::new(|| {
    HashMap::from([
        (
            vkey::Key::B.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_battery_limit();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::P.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_perf_mode();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::R.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_refresh_rate();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::T.into(),
            KeyEventAction::new(
                Box::new(|| {
                    toggle_trackpad();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::V.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().toggle_vc();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::F1.into(),
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
            vkey::Key::F2.into(),
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
            vkey::Key::F3.into(),
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
            vkey::Key::F4.into(),
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
            vkey::Key::F5.into(),
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
            vkey::Key::F6.into(),
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
            vkey::Key::F7.into(),
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
            vkey::Key::F8.into(),
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
            vkey::Key::F9.into(),
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
            vkey::Key::F10.into(),
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
            vkey::Key::F11.into(),
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
            vkey::Key::F12.into(),
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
            razer_key::Key::Mic.into(),
            KeyEventAction::new(
                Box::new(|| {
                    audio::toggle_audio_mute(AudioType::Mic);
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Trackpad.into(),
            KeyEventAction::new(
                Box::new(|| {
                    toggle_trackpad();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Perf.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_perf_mode();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::CoPilot.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_rgb_mode();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Home.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(36)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Up.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(38)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::PgUp.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(33)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Left.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(37)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Right.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(39)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::End.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(35)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Down.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(40)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::PgDn.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(34)]).trigger();
                }),
                vec![],
            ),
        ),
    ])
});
