use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::core::shared_state::{SCREEN_ADJUSTING, SCREEN_TARGET_LVL};
use crate::ui::app::app;
use crate::ui::app_events::OsdEvent;
use tracing::{debug, warn};

pub struct BrightnessWorker {
    tx: Sender<()>,
}

impl BrightnessWorker {
    pub fn new() -> Self {
        let (tx, rx) = channel::<()>();

        // Spawn the dedicated background thread
        std::thread::spawn(move || {
            Self::worker_loop(rx);
        });

        Self { tx }
    }

    pub fn set_screen_brightness(&self, new_level: u8) {
        let discrete_lvl = (new_level as f64 / 10.0).round() as u8 * 10;
        app().send(OsdEvent::ScreenBrightness(discrete_lvl).into());
        debug!(level = discrete_lvl, "Queuing screen brightness change");
        SCREEN_TARGET_LVL.store(discrete_lvl, Ordering::SeqCst);
        let _ = self.tx.send(());
    }

    pub fn adjust_screen_brightness(&self, change: i8) {
        let current = { SCREEN_TARGET_LVL.load(Ordering::SeqCst) } as i8;
        let new_val = (current + change).clamp(0, 100) as u8;
        self.set_screen_brightness(new_val);
    }

    fn worker_loop(rx: Receiver<()>) {
        let mut last_processed_lvl: u8 = 101;
        // Block on the channel waiting for "pokes"
        while rx.recv().is_ok() {
            while rx.try_recv().is_ok() {} // drain queue in channel to prevent extra updates
            // Get the latest target
            let target = SCREEN_TARGET_LVL.load(Ordering::SeqCst);

            // skip if target has not changed
            if target == last_processed_lvl {
                debug!(
                    level = last_processed_lvl,
                    "Brightness target unchanged; skipping write"
                );
                continue;
            }
            SCREEN_ADJUSTING.fetch_add(1, Ordering::SeqCst);
            set_hardware_brightness(target as u32);
            last_processed_lvl = target;
        }
    }
}

fn set_hardware_brightness(percentage: u32) -> bool {
    let script = format!(
        "$wmi = Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods;\
         Invoke-CimMethod -InputObject $wmi -MethodName WmiSetBrightness -Arguments @{{Timeout = 0; Brightness = {}}}",
        percentage
    );

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match output {
        Ok(out) => {
            if out.status.success() {
                true
            } else {
                let err_msg = String::from_utf8_lossy(&out.stderr);
                warn!("OS Pipeline Error: {}", err_msg);
                false
            }
        }
        Err(e) => {
            warn!("Failed to execute process payload: {}", e);
            false
        }
    }
}
