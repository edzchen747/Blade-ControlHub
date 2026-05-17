use std::sync::atomic::Ordering;

use crate::{
    config,
    ui::{app::app, app_events::AppEvent},
    win::input::{
        key_map::{FN_PRESSED, KEY_MAP},
        scancode,
    },
};
use hidapi::{HidApi, HidDevice};

pub struct HidApiListener {
    device_pid: u16,
}

impl HidApiListener {
    pub fn new(device_pid: u16) -> Self {
        Self { device_pid }
    }
    pub fn start(&self) {
        let api = HidApi::new().expect("Failed to init HID API");
        let mut opened_count = 0;

        for device_info in api
            .device_list()
            .filter(|d| d.vendor_id() == config::RAZER_VID && d.product_id() == self.device_pid)
        {
            let path = device_info.path().to_owned();
            let iface_num = device_info.interface_number();

            if let Ok(device_api) = api.open_path(&path) {
                // Test if we can read (check for Access Denied)
                let mut test_buf = [0u8; 16];
                let read_result = device_api.read_timeout(&mut test_buf, 5);
                if read_result.is_ok() || !read_result.unwrap_err().to_string().contains("denied") {
                    opened_count += 1;
                    println!("Opened Interface {} (Path: {:?})", iface_num, path);
                    self.spawn_special_key_listener_thread(
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
    }

    fn spawn_special_key_listener_thread(
        &self,
        device_api: HidDevice,
        iface_num: i32,
        path: String,
    ) {
        std::thread::spawn(move || {
            let mut buf = [0u8; 16];
            // Close interfaces as soon as we detect they are likely not the target
            while let Ok(len) = device_api.read(&mut buf) {
                if len == 0 {
                    println!("noise?");
                    break;
                }
                if buf[0] == 0x04 {
                    // Razer special key events
                    match buf[1] {
                        0x0a => FN_PRESSED.store(true, Ordering::SeqCst),
                        0x00 => FN_PRESSED.store(false, Ordering::SeqCst),
                        _ => {
                            app().send(AppEvent::RazerKeyCode(buf[1]));
                            let key = scancode::Key::from(buf[1]);
                            if let Some(action) = KEY_MAP.get(&key) {
                                let _ = action.execute();
                            } else {
                                println!("Unmapped keycode detected: {:#04x}", buf[1]);
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
}
