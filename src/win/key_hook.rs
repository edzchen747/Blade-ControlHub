use super::actions;
use crate::razer;
use crate::razer::device_handle::device;
use crate::ui::app_events::AppEvent;
use crate::ui::tray_app::tray_app;
use crate::win::actions::get_trackpad_state;
use actions::AudioType;

use hidapi::{HidApi, HidDevice};
use once_cell::sync::Lazy;
use rdev::{Event, EventType, Key, grab, simulate};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

pub static FN_PRESSED: AtomicBool = AtomicBool::new(false);
pub static ALT_PRESSED: AtomicBool = AtomicBool::new(false);

pub struct KeyCombo([Option<Key>; 4]);

impl KeyCombo {
    fn new(input: &[Key]) -> Self {
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

pub struct KeyConfig {
    pub normal: &'static str,
    pub special: &'static str,
    pub default_original: bool,
    pub special_keys: KeyCombo,
    pub func: Option<Box<dyn Fn() + Send + Sync>>,
}

impl KeyConfig {
    pub fn trigger(&self) {
        self.special_keys.trigger();
        if let Some(logic) = self.func.as_ref() {
            logic();
        }
    }
}

pub static KEY_MAP: Lazy<HashMap<Key, KeyConfig>> = Lazy::new(|| {
    HashMap::from([
        (
            Key::KeyR,
            KeyConfig {
                normal: "R",
                special: "FN + R",
                default_original: true,
                special_keys: KeyCombo::new(&[]),
                func: Some(Box::new(|| {
                    device().cycle_refresh_rate();
                })),
            },
        ),
        (
            Key::KeyT,
            KeyConfig {
                normal: "T",
                special: "FN + T",
                default_original: true,
                special_keys: KeyCombo::new(&[Key::MetaLeft, Key::ControlLeft, Key::Unknown(135)]),
                func: Some(Box::new(|| {
                    tray_app().send(AppEvent::Trackpad(get_trackpad_state()))
                })),
            },
        ),
        (
            Key::KeyV,
            KeyConfig {
                normal: "V",
                special: "FN + V",
                default_original: true,
                special_keys: KeyCombo::new(&[]),
                func: Some(Box::new(|| device().toggle_vc())),
            },
        ),
        (
            Key::KeyB,
            KeyConfig {
                normal: "B",
                special: "FN + B",
                default_original: true,
                special_keys: KeyCombo::new(&[]),
                func: None,
            },
        ),
        (
            Key::KeyP,
            KeyConfig {
                normal: "P",
                special: "FN + P",
                default_original: true,
                special_keys: KeyCombo::new(&[]),
                func: Some(Box::new(|| device().cycle_perf_mode())),
            },
        ),
        (
            Key::F1,
            KeyConfig {
                normal: "Mute",
                special: "F1",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(173)]),
                func: None,
            },
        ),
        (
            Key::F2,
            KeyConfig {
                normal: "Vol-",
                special: "F2",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(174)]),
                func: None,
            },
        ),
        (
            Key::F3,
            KeyConfig {
                normal: "Vol+",
                special: "F3",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(175)]),
                func: None,
            },
        ),
        (
            Key::F4,
            KeyConfig {
                normal: "Project",
                special: "F4",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::MetaLeft, Key::KeyP]),
                func: None,
            },
        ),
        (
            Key::F5,
            KeyConfig {
                normal: "|◀◀",
                special: "F5",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(177)]),
                func: None,
            },
        ),
        (
            Key::F6,
            KeyConfig {
                normal: "▶||",
                special: "F6",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(179)]),
                func: None,
            },
        ),
        (
            Key::F7,
            KeyConfig {
                normal: "▶▶|",
                special: "F7",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(176)]),
                func: None,
            },
        ),
        (
            Key::F8,
            KeyConfig {
                normal: "Brightness-",
                special: "F8",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: Some(Box::new(|| device().adjust_screen_brightness(-10))),
            },
        ),
        (
            Key::F9,
            KeyConfig {
                normal: "Brightness+",
                special: "F9",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: Some(Box::new(|| device().adjust_screen_brightness(10))),
            },
        ),
        (
            Key::F10,
            KeyConfig {
                normal: "Keyboard-",
                special: "F10",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: Some(Box::new(|| device().keyboard_light_down())),
            },
        ),
        (
            Key::F11,
            KeyConfig {
                normal: "Keyboard+",
                special: "F11",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: Some(Box::new(|| device().keyboard_light_up())),
            },
        ),
        (
            Key::F12,
            KeyConfig {
                normal: "prt sc",
                special: "F12",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::PrintScreen]),
                func: None,
            },
        ),
    ])
});

pub static RAZER_KEY_MAP: Lazy<HashMap<u8, KeyConfig>> = Lazy::new(|| {
    HashMap::from([
        (
            0x0a,
            KeyConfig {
                normal: "fn",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: Some(Box::new(|| FN_PRESSED.store(true, Ordering::SeqCst))),
            },
        ),
        (
            0x00,
            KeyConfig {
                normal: "clear",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: Some(Box::new(|| FN_PRESSED.store(false, Ordering::SeqCst))),
            },
        ),
        (
            0xd4,
            KeyConfig {
                normal: "Mic Mute Toggle",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(157)]),
                func: Some(Box::new(|| actions::toggle_audio_mute(AudioType::Mic))),
            },
        ),
        (
            0xdd,
            KeyConfig {
                normal: "Trackpad Toggle",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::MetaLeft, Key::ControlLeft, Key::Unknown(135)]),
                func: Some(Box::new(|| {
                    tray_app().send(AppEvent::Trackpad(get_trackpad_state()))
                })),
            },
        ),
        (
            0xd3,
            KeyConfig {
                normal: "Performance Mode Cycle",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: Some(Box::new(|| device().cycle_perf_mode())),
            },
        ),
        (
            0x24,
            KeyConfig {
                normal: "M1",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: None,
            },
        ),
        (
            0x25,
            KeyConfig {
                normal: "M2",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: None,
            },
        ),
        (
            0x26,
            KeyConfig {
                normal: "M3",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: None,
            },
        ),
        (
            0x27,
            KeyConfig {
                normal: "M4",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: None,
            },
        ),
        (
            0x03,
            KeyConfig {
                normal: "Game Mode Toggle",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: None,
            },
        ),
        (
            0xd2,
            KeyConfig {
                normal: "CoPilot",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[]),
                func: Some(Box::new(|| device().cycle_rgb_mode())),
            },
        ),
        (
            0xd5,
            KeyConfig {
                normal: "home",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(36)]),
                func: None,
            },
        ),
        (
            0xd6,
            KeyConfig {
                normal: "up",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(38)]),
                func: None,
            },
        ),
        (
            0xd7,
            KeyConfig {
                normal: "pg up",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(33)]),
                func: None,
            },
        ),
        (
            0xd8,
            KeyConfig {
                normal: "left",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(37)]),
                func: None,
            },
        ),
        (
            0xd9,
            KeyConfig {
                normal: "right",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(39)]),
                func: None,
            },
        ),
        (
            0xda,
            KeyConfig {
                normal: "end",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(35)]),
                func: None,
            },
        ),
        (
            0xdb,
            KeyConfig {
                normal: "down",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(40)]),
                func: None,
            },
        ),
        (
            0xdc,
            KeyConfig {
                normal: "pg dn",
                special: "",
                default_original: false,
                special_keys: KeyCombo::new(&[Key::Unknown(34)]),
                func: None,
            },
        ),
    ])
});

pub fn init_keyboard_hooks(device_pid: u16) {
    let api = HidApi::new().expect("Failed to init HID API");
    let mut opened_count = 0;

    for device_info in api
        .device_list()
        .filter(|d| d.vendor_id() == razer::RAZER_VID && d.product_id() == device_pid)
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
            if len > 0 {
                if buf[0] == 0x04 {
                    if let Some(config) = RAZER_KEY_MAP.get(&buf[1]) {
                        println!("{} DETECTED!", config.normal);
                        config.trigger();
                    }
                } else if buf[0] == 0x01 {
                    // pass - standard key codes
                } else {
                    break;
                }
            } else {
                println!("noise?");
                break;
            };
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
    if let EventType::KeyPress(key) = event.event_type {
        if let Some(config) = KEY_MAP.get(&key) {
            let is_fn = FN_PRESSED.load(Ordering::SeqCst);
            let is_alt = ALT_PRESSED.load(Ordering::SeqCst);

            if is_alt {
                // Alt modifier key should use F keys even if they are set to defualt multimedia (e.g. Alt F4)
                Some(event)
            } else if is_fn {
                println!("{} DETECTED!", config.special);
                if config.default_original {
                    config.trigger();
                    None
                } else {
                    Some(event)
                }
            } else {
                println!("{} DETECTED!", config.normal);
                if config.default_original {
                    Some(event)
                } else {
                    config.trigger();
                    None
                }
            }
        } else {
            if key == Key::Alt {
                println! {"alt pressed"}
                ALT_PRESSED.store(true, Ordering::SeqCst);
            }
            Some(event)
        }
    } else {
        if let EventType::KeyRelease(key) = event.event_type
            && key == Key::Alt
        {
            println! {"released alt"}
            ALT_PRESSED.store(false, Ordering::SeqCst);
        }
        Some(event)
    }
}
