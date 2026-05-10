use crate::config;
use crate::razer::device_handle::device;
use crate::win::audio::{self, AudioType};
use crate::win::input::key;
use crate::win::input::trackpad::toggle_trackpad;

use hidapi::{HidApi, HidDevice};
use once_cell::sync::Lazy;
use rdev::{Event, EventType, Key, grab, simulate};
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

pub fn init_keyboard_hooks(device_pid: u16) {
    let api = HidApi::new().expect("Failed to init HID API");
    let mut opened_count = 0;

    for device_info in api
        .device_list()
        .filter(|d| d.vendor_id() == config::RAZER_VID && d.product_id() == device_pid)
    {
        let path = device_info.path().to_owned();
        let iface_num = device_info.interface_number();

        if let Ok(device_api) = api.open_path(&path) {
            // Test if we can read (check for Access Denied)
            let mut test_buf = [0u8; 16];
            if device_api.read_timeout(&mut test_buf, 5).is_ok()
                || !device_api
                    .read_timeout(&mut test_buf, 5)
                    .unwrap_err()
                    .to_string()
                    .contains("denied")
            {
                opened_count += 1;
                println!("Opened Interface {} (Path: {:?})", iface_num, path);
                spawn_special_key_listener_thread(
                    device_api,
                    iface_num,
                    path.to_string_lossy().into_owned(),
                );
            }
        } else {
            println!("LOCKED: Interface {}", iface_num);
        }
    }

    assert!(opened_count > 0, "No Razer interfaces were accessible.");

    println!(
        "\n--- {} HID listeners active. Keyboard (special key) hook thread started. ---",
        opened_count
    );
    spawn_standard_key_listener_thread();
}

pub fn spawn_special_key_listener_thread(device_api: HidDevice, iface_num: i32, path: String) {
    thread::spawn(move || {
        let mut buf = [0u8; 16];
        // Close interfaces as soon as we detect they are likely not the target
        while let Ok(len) = device_api.read(&mut buf) {
            if !(len > 0) {
                println!("noise?");
                break;
            }
            if buf[0] == 0x04 {
                // Razer special key events
                match buf[1] {
                    0x0a => FN_PRESSED.store(true, Ordering::SeqCst),
                    0x00 => FN_PRESSED.store(false, Ordering::SeqCst),
                    _ => {
                        let key = key::Key::from(buf[1]);
                        if let Some(action) = KEY_MAP.get(&key) {
                            let _ = action.execute();
                        }
                    }
                }
            } else {
                break;
            }
        }
        println!("Closed Interface {} (Path: {:?})", iface_num, path);
    });
}

pub fn spawn_standard_key_listener_thread() {
    thread::spawn(|| {
        println!("\n--- Keyboard (standard key) grab thread started. ---");
        if let Err(e) = grab(standard_key_callback) {
            eprintln!("Keyboard grab failed: {:?}", e);
        }
    });
}

fn standard_key_callback(event: Event) -> Option<Event> {
    if let EventType::KeyPress(rdev_key) = event.event_type {
        if rdev_key == Key::Alt {
            ALT_PRESSED.store(true, Ordering::SeqCst);
        }

        let key = key::Key::from(rdev_key);
        if key != key::Key::Unknown(0) {
            if let Some(event_action) = KEY_MAP.get(&key) {
                match event_action.execute() {
                    true => return None,
                    false => (),
                }
            }
        }
    } else if let EventType::KeyRelease(key) = event.event_type {
        if key == Key::Alt {
            ALT_PRESSED.store(false, Ordering::SeqCst);
        }
    }
    Some(event)
}
