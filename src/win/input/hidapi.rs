use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

use crate::{
    config,
    core::shared_state::{FN_PRESSED, KEYMAP_LISTENING},
    error::{AppError, AppResult},
    win::input::{key_map::KEY_MAP, razer_key},
};
use hidapi::{HidApi, HidDevice};
use tracing::{info, warn};

const HID_READ_TIMEOUT_MS: i32 = 250;
static HID_LISTENERS_RUNNING: AtomicBool = AtomicBool::new(false);
static HID_LISTENER_THREADS: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());

pub struct HidApiListener {
    device_pid: u16,
}

impl HidApiListener {
    pub fn new(device_pid: u16) -> Self {
        Self { device_pid }
    }
    pub fn start(&self) -> AppResult<()> {
        if HID_LISTENERS_RUNNING.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let api = match HidApi::new() {
            Ok(api) => api,
            Err(error) => {
                HID_LISTENERS_RUNNING.store(false, Ordering::SeqCst);
                return Err(AppError::HidApi(error.to_string()));
            }
        };
        let mut listener_count = 0;

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
                if interface_is_readable_or_not_denied(read_result) {
                    info!(interface = iface_num, path = %path.to_string_lossy(), "HID interface opened");
                    match self.spawn_special_key_listener_thread(
                        device_api,
                        iface_num,
                        path.to_string_lossy().into_owned(),
                    ) {
                        Ok(()) => listener_count += 1,
                        Err(error) => {
                            warn!(interface = iface_num, %error, "Failed to start HID listener thread");
                        }
                    }
                }
            } else {
                warn!(
                    interface = iface_num,
                    "HID interface locked by OS or another process"
                );
            }
        }

        if listener_count == 0 {
            HID_LISTENERS_RUNNING.store(false, Ordering::SeqCst);
            return Err(AppError::NoInterfacesAccessible);
        }

        info!(listener_count, "HID listener threads active");
        Ok(())
    }

    pub fn stop() {
        HID_LISTENERS_RUNNING.store(false, Ordering::SeqCst);
        FN_PRESSED.store(false, Ordering::SeqCst);
        join_hid_listener_threads();
    }

    fn spawn_special_key_listener_thread(
        &self,
        device_api: HidDevice,
        iface_num: i32,
        path: String,
    ) -> std::io::Result<()> {
        let handle = thread::Builder::new()
            .name(format!("blade-hid-iface-{iface_num}"))
            .spawn(move || run_special_key_listener(device_api, iface_num, path))?;
        hid_listener_threads().push(handle);
        Ok(())
    }
}

fn hid_listener_threads() -> MutexGuard<'static, Vec<JoinHandle<()>>> {
    HID_LISTENER_THREADS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_hid_listener_threads() {
    let current_thread_id = thread::current().id();
    let mut handles = {
        let mut threads = hid_listener_threads();
        std::mem::take(&mut *threads)
    };

    for handle in handles.drain(..) {
        if handle.thread().id() == current_thread_id {
            warn!("Skipping join of current HID listener thread during shutdown");
            continue;
        }

        if handle.join().is_err() {
            warn!("HID listener thread panicked during shutdown");
        }
    }
}

fn interface_is_readable_or_not_denied(read_result: hidapi::HidResult<usize>) -> bool {
    match read_result {
        Ok(_) => true,
        Err(error) => !error.to_string().contains("denied"),
    }
}

fn run_special_key_listener(device_api: HidDevice, iface_num: i32, path: String) {
    let mut buf = [0u8; 16];
    // Close interfaces as soon as we detect they are likely not the target
    while HID_LISTENERS_RUNNING.load(Ordering::SeqCst) {
        match device_api.read_timeout(&mut buf, HID_READ_TIMEOUT_MS) {
            Ok(0) => continue,
            Ok(len) => match parse_special_key_report(&buf[..len]) {
                SpecialKeyReport::KeyCode(key_code) => handle_razer_special_key(key_code),
                SpecialKeyReport::TooShort { len } => {
                    warn!(
                        interface = iface_num,
                        report_len = len,
                        "HID special-key report was too short, ignoring"
                    );
                }
                SpecialKeyReport::UnexpectedReportId(report_id) => {
                    info!(
                        interface = iface_num,
                        report_id, "HID interface produced an unrelated report, closing listener"
                    );
                    break;
                }
            },
            Err(error) => {
                warn!(interface = iface_num, ?error, "HID listener read failed");
                break;
            }
        }
    }
    info!(interface = iface_num, path = %path, "HID interface listener closed");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpecialKeyReport {
    KeyCode(u8),
    TooShort { len: usize },
    UnexpectedReportId(u8),
}

fn parse_special_key_report(report: &[u8]) -> SpecialKeyReport {
    match report {
        [0x04, key_code, ..] => SpecialKeyReport::KeyCode(*key_code),
        [0x04] => SpecialKeyReport::TooShort { len: report.len() },
        [report_id, ..] => SpecialKeyReport::UnexpectedReportId(*report_id),
        [] => SpecialKeyReport::TooShort { len: 0 },
    }
}

fn handle_razer_special_key(key_code: u8) {
    match key_code {
        0x0a => FN_PRESSED.store(true, Ordering::SeqCst),
        0x00 => FN_PRESSED.store(false, Ordering::SeqCst),
        _ => {
            let key = razer_key::Key::from(key_code);
            crate::ipc::server::record_razer_key_code(key_code);
            if !KEYMAP_LISTENING.load(Ordering::SeqCst) {
                if let Some(action) = KEY_MAP.get(&key.into()) {
                    let _ = action.execute();
                } else {
                    warn!(keycode = key_code, "Unmapped Razer keycode received");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_key_report_extracts_key_code() {
        assert_eq!(
            parse_special_key_report(&[0x04, 0x0a, 0xff]),
            SpecialKeyReport::KeyCode(0x0a)
        );
    }

    #[test]
    fn special_key_report_rejects_short_special_report() {
        assert_eq!(
            parse_special_key_report(&[0x04]),
            SpecialKeyReport::TooShort { len: 1 }
        );
    }

    #[test]
    fn special_key_report_classifies_unrelated_report_id() {
        assert_eq!(
            parse_special_key_report(&[0x02, 0x0a]),
            SpecialKeyReport::UnexpectedReportId(0x02)
        );
    }

    #[test]
    fn stop_clears_hid_listener_running_state_and_fn_state() {
        HID_LISTENERS_RUNNING.store(true, Ordering::SeqCst);
        FN_PRESSED.store(true, Ordering::SeqCst);

        HidApiListener::stop();

        assert!(!HID_LISTENERS_RUNNING.load(Ordering::SeqCst));
        assert!(!FN_PRESSED.load(Ordering::SeqCst));
    }

    #[test]
    fn stop_drains_finished_hid_listener_threads() {
        hid_listener_threads().push(thread::spawn(|| {}));
        HID_LISTENERS_RUNNING.store(true, Ordering::SeqCst);

        HidApiListener::stop();

        assert!(hid_listener_threads().is_empty());
    }
}
