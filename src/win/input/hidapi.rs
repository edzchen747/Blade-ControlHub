use std::sync::atomic::Ordering;

use crate::{
    config,
    core::shared_state::{FN_PRESSED, KEYMAP_LISTENING},
    error::{AppError, AppResult},
    ui::{app::app, app_events::AppEvent},
    win::input::{key_map::KEY_MAP, razer_key},
};
use hidapi::{HidApi, HidDevice};
use tracing::{info, warn};

pub struct HidApiListener {
    device_pid: u16,
}

impl HidApiListener {
    pub fn new(device_pid: u16) -> Self {
        Self { device_pid }
    }
    pub fn start(&self) -> AppResult<()> {
        let api = HidApi::new().map_err(|e| AppError::HidApi(e.to_string()))?;
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
                    info!(interface = iface_num, path = %path.to_string_lossy(), "HID interface opened");
                    self.spawn_special_key_listener_thread(
                        device_api,
                        iface_num,
                        path.to_string_lossy().into_owned(),
                    );
                }
            } else {
                warn!(
                    interface = iface_num,
                    "HID interface locked by OS or another process"
                );
            }
        }

        if opened_count == 0 {
            return Err(AppError::NoInterfacesAccessible);
        }

        info!(listener_count = opened_count, "HID listener threads active");
        Ok(())
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
                    warn!(
                        interface = iface_num,
                        "HID read returned zero bytes (noise), closing interface"
                    );
                    break;
                }
                if buf[0] == 0x04 {
                    // Razer special key events
                    match buf[1] {
                        0x0a => FN_PRESSED.store(true, Ordering::SeqCst),
                        0x00 => FN_PRESSED.store(false, Ordering::SeqCst),
                        _ => {
                            let key = razer_key::Key::from(buf[1]);
                            if !KEYMAP_LISTENING.load(Ordering::SeqCst) {
                                if let Some(action) = KEY_MAP.get(&key.into()) {
                                    let _ = action.execute();
                                } else {
                                    warn!(keycode = buf[1], "Unmapped Razer keycode received");
                                }
                            } else {
                                app().send(AppEvent::RazerKeyCode(buf[1]));
                            }
                        }
                    }
                } else {
                    info!("Something else {:?}", buf[2]);
                    break;
                }
            }
            info!(interface = iface_num, path = %path, "HID interface listener closed");
        });
    }
}
