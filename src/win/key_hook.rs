use super::actions;
use actions::AudioType;
use crate::razer;

use std::collections::HashMap;
use rdev::{grab, simulate, EventType, Event, Key};
use std::sync::atomic::{AtomicBool, Ordering};
use once_cell::sync::Lazy;
use std::time::Duration;
use std::thread;
use anyhow::Result;
use hidapi::{HidApi, HidDevice};


pub static FN_PRESSED: AtomicBool = AtomicBool::new(false);

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
    pub normal_label: &'static str,
    pub fn_label: &'static str,
    pub fn_blocks_event: bool,
    pub special_keys: KeyCombo,
    pub func: Option<Box<dyn Fn() -> Result<()> + Send + Sync>>,
}

impl KeyConfig {
    pub fn trigger(&self) {
        self.special_keys.trigger();
        if let Some(logic) = self.func.as_ref() {
            let _ = logic();
        }
    }
}

pub static KEY_MAP: Lazy<HashMap<Key, KeyConfig>> = Lazy::new(|| {
    HashMap::from([
        (Key::KeyR, KeyConfig { normal_label: "R", fn_label: "FN + R", fn_blocks_event: true, special_keys: KeyCombo::new(&[]), func: None }),
        (Key::KeyT, KeyConfig { normal_label: "T", fn_label: "FN + T", fn_blocks_event: true, special_keys: KeyCombo::new(&[Key::MetaLeft, Key::ControlLeft, Key::Unknown(135)]), func: None }),
        (Key::KeyV, KeyConfig { normal_label: "V", fn_label: "FN + V", fn_blocks_event: true, special_keys: KeyCombo::new(&[]), func: Some(Box::new(|| razer::actions::toggle_vc_brightness())) }),
        (Key::KeyB, KeyConfig { normal_label: "B", fn_label: "FN + B", fn_blocks_event: true, special_keys: KeyCombo::new(&[]), func: None }),
        (Key::KeyP, KeyConfig { normal_label: "P", fn_label: "FN + P", fn_blocks_event: true, special_keys: KeyCombo::new(&[]), func: None }),
        (Key::F1, KeyConfig { normal_label: "Mute", fn_label: "F1", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(173)]), func: None }),
        (Key::F2, KeyConfig { normal_label: "Vol-", fn_label: "F2", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(174)]), func: None }),
        (Key::F3, KeyConfig { normal_label: "Vol+", fn_label: "F3", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(175)]), func: None }),
        (Key::F4, KeyConfig { normal_label: "Project", fn_label: "F4", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::MetaLeft, Key::KeyP]), func: None }),
        (Key::F5, KeyConfig { normal_label: "|◀◀", fn_label: "F5", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(177)]), func: None }),
        (Key::F6, KeyConfig { normal_label: "▶||", fn_label: "F6", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(179)]), func: None }),
        (Key::F7, KeyConfig { normal_label: "▶▶|", fn_label: "F7", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(176)]), func: None }),
        (Key::F8, KeyConfig { normal_label: "Brightness-", fn_label: "F8", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: Some(Box::new(|| actions::adjust_brightness(-10) )) }),
        (Key::F9, KeyConfig { normal_label: "Brightness+", fn_label: "F9", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: Some(Box::new(|| actions::adjust_brightness(10) )) }),
        (Key::F10, KeyConfig { normal_label: "Keyboard-", fn_label: "F10", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: Some(Box::new(|| razer::actions::keyboard_light(false))) }),
        (Key::F11, KeyConfig { normal_label: "Keyboard+", fn_label: "F11", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: Some(Box::new(|| razer::actions::keyboard_light(true))) }),
        (Key::F12, KeyConfig { normal_label: "prt sc", fn_label: "F12", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::PrintScreen]), func: None }),
    ])
});

pub static RAZER_KEY_MAP: Lazy<HashMap<u8, KeyConfig>> = Lazy::new(|| {
    HashMap::from([
        (0x0a, KeyConfig { normal_label: "fn", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: Some(Box::new(|| {FN_PRESSED.store(true, Ordering::SeqCst); Ok(()) } )) }),
        (0x00, KeyConfig { normal_label: "clear", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: Some(Box::new(|| {FN_PRESSED.store(false, Ordering::SeqCst); Ok(()) } )) }),
        (0xd4, KeyConfig { normal_label: "Mic Mute Toggle", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(157)]), func: Some(Box::new(|| actions::toggle_audio_mute(AudioType::Mic))) }),
        (0xdd, KeyConfig { normal_label: "Trackpad Toggle", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::MetaLeft, Key::ControlLeft, Key::Unknown(135)]), func: None }),
        (0xd3, KeyConfig { normal_label: "Performance Mode Cycle", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: None }),
        (0x24, KeyConfig { normal_label: "M1", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: None }),
        (0x25, KeyConfig { normal_label: "M2", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: None }),
        (0x26, KeyConfig { normal_label: "M3", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: None }),
        (0x27, KeyConfig { normal_label: "M4", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: None }),
        (0x03, KeyConfig { normal_label: "Game Mode Toggle", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: None }),
        (0xd2, KeyConfig { normal_label: "CoPilot", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[]), func: Some(Box::new(|| razer::actions::cycle_rgb_mode())) }),
        (0xd5, KeyConfig { normal_label: "home", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(36)]), func: None }),
        (0xd6, KeyConfig { normal_label: "up", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(38)]), func: None }),
        (0xd7, KeyConfig { normal_label: "pg up", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(33)]), func: None }),
        (0xd8, KeyConfig { normal_label: "left", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(37)]), func: None }),
        (0xd9, KeyConfig { normal_label: "right", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(39)]), func: None }),
        (0xda, KeyConfig { normal_label: "end", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(35)]), func: None }),
        (0xdb, KeyConfig { normal_label: "down", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(40)]), func: None }),
        (0xdc, KeyConfig { normal_label: "pg dn", fn_label: "", fn_blocks_event: false, special_keys: KeyCombo::new(&[Key::Unknown(34)]), func: None }),
    ])
});

pub fn init_keyboard_hooks(device_pid: u16) -> anyhow::Result<()> {
    let api = HidApi::new().expect("Failed to init HID API");
    let mut opened_count = 0;

    for device_info in api.device_list().filter(|d| d.vendor_id() == razer::RAZER_VID && d.product_id() == device_pid) {
        let path = device_info.path().to_owned();
        let iface_num = device_info.interface_number();

        if let Ok(device_api) = api.open_path(&path) {
            // Test if we can read (check for Access Denied)
            let mut test_buf = [0u8; 16];
            if device_api.read_timeout(&mut test_buf, 5).is_ok() ||
               !device_api.read_timeout(&mut test_buf, 5).unwrap_err().to_string().contains("denied")
            {
                opened_count += 1;
                println!("SUCCESS: Opened Interface {} (Path: {:?})", iface_num, path);
                spawn_special_key_listener_thread(device_api);
            }
        } else {
            println!("LOCKED: Interface {}", iface_num);
        }
    }

    if opened_count == 0 {
        return Err(anyhow::anyhow!("No Razer interfaces were accessible."));
    }

    println!("\n--- {} HID listeners active. Keyboard (special key) hook thread started. ---", opened_count);
    spawn_standard_key_listener_thread();
    spawn_update_key_indicators_thread();
    Ok(())
}

pub fn spawn_special_key_listener_thread(device_api: HidDevice) {
    thread::spawn(move || {
        let mut buf = [0u8; 16];
        let last_mic_muted = actions::is_audio_muted(AudioType::Mic);
        let last_speakers_muted = actions::is_audio_muted(AudioType::Speakers);
        let _ = razer::actions::set_mute_indicator(AudioType::Mic, last_mic_muted);
        let _ = razer::actions::set_mute_indicator(AudioType::Speakers, last_speakers_muted);

        loop {
            if let Ok(len) = device_api.read(&mut buf) {
                if len > 0 && buf[0] == 0x04 {
                    if let Some(config) = RAZER_KEY_MAP.get(&buf[1]) {
                        println!("{} DETECTED!", config.normal_label);
                        config.trigger();
                    }
                }
            };

            // Keep the loop fast but not burning 100% CPU
            thread::sleep(Duration::from_millis(50));
        }
    });
}

pub fn spawn_standard_key_listener_thread() {
    thread::spawn(|| {
        println!("\n--- Keyboard (standard key) grab thread started. ---");
        if let Err(e) = grab(move |event| {
            standard_key_callback(event)
        }) {
            eprintln!("Keyboard grab failed: {:?}", e);
        }
    });
}

fn standard_key_callback(event: Event) -> Option<Event> {
    if let EventType::KeyPress(key) = event.event_type {
        if let Some(config) = KEY_MAP.get(&key) {
            let is_fn = FN_PRESSED.load(Ordering::SeqCst);
            
            if is_fn {
                println!("{} DETECTED!", config.fn_label);
                if config.fn_blocks_event {
                    config.trigger();
                    None
                } else {
                    Some(event)
                }
            } else {
                println!("{} DETECTED!", config.normal_label);
                if config.fn_blocks_event {
                    Some(event)
                } else {
                    config.trigger();
                    None
                }
            }
        } else {
            Some(event)
        }
    } else {
        Some(event)
    }
}

fn spawn_update_key_indicators_thread() {
    thread::spawn(|| {
        println!("\n--- Keyboard indicators update thread started. ---");
        let mut last_mic_muted = actions::is_audio_muted(AudioType::Mic);
        let mut last_speakers_muted = actions::is_audio_muted(AudioType::Speakers);
        let _ = razer::actions::set_mute_indicator(AudioType::Mic, last_mic_muted);
        let _ = razer::actions::set_mute_indicator(AudioType::Speakers, last_speakers_muted);

        loop {
            let mic_muted = actions::is_audio_muted(AudioType::Mic);
            if mic_muted != last_mic_muted {
                let _ = razer::actions::set_mute_indicator(AudioType::Mic, mic_muted);
            };
            last_mic_muted = mic_muted;

            let speakers_muted = actions::is_audio_muted(AudioType::Speakers);
            if speakers_muted != last_speakers_muted {
                let _ = razer::actions::set_mute_indicator(AudioType::Speakers, speakers_muted);
            };
            last_speakers_muted = speakers_muted;

            thread::sleep(Duration::from_millis(100));
        }
    });
}
