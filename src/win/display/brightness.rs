use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};

use crate::core::shared_state::{SCREEN_ADJUSTING, SCREEN_TARGET_LVL};
use crate::ui::app::app;
use crate::ui::app_events::OsdEvent;
use tracing::{debug, warn};

pub struct BrightnessWorker {
    tx: Option<Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl BrightnessWorker {
    pub fn new() -> Self {
        let (tx, rx) = channel::<()>();

        let worker = match thread::Builder::new()
            .name("blade-brightness-worker".to_string())
            .spawn(move || {
                Self::worker_loop(rx);
            }) {
            Ok(worker) => Some(worker),
            Err(error) => {
                warn!(%error, "Failed to start brightness worker thread");
                None
            }
        };

        Self {
            tx: Some(tx),
            worker,
        }
    }

    pub fn set_screen_brightness(&self, new_level: u8) {
        let discrete_lvl = (new_level as f64 / 10.0).round() as u8 * 10;
        app(OsdEvent::ScreenBrightness(discrete_lvl).into());
        debug!(level = discrete_lvl, "Queuing screen brightness change");
        SCREEN_TARGET_LVL.store(discrete_lvl, Ordering::SeqCst);
        let Some(tx) = &self.tx else {
            warn!("Brightness write requested after worker sender was closed");
            return;
        };

        if let Err(error) = tx.send(()) {
            warn!(
                ?error,
                "Brightness worker is unavailable; dropping brightness update"
            );
        }
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
            let _adjusting = BrightnessAdjustmentGuard::new();
            set_hardware_brightness(target as u32);
            last_processed_lvl = target;
        }
    }

    fn join_worker(&mut self) {
        drop(self.tx.take());

        let Some(worker) = self.worker.take() else {
            return;
        };

        if worker.thread().id() == thread::current().id() {
            warn!("Skipping join of current brightness worker thread during shutdown");
            self.worker = Some(worker);
            return;
        }

        if worker.join().is_err() {
            warn!("Brightness worker thread panicked during shutdown");
        }
    }
}

impl Default for BrightnessWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for BrightnessWorker {
    fn drop(&mut self) {
        self.join_worker();
    }
}

struct BrightnessAdjustmentGuard;

impl BrightnessAdjustmentGuard {
    fn new() -> Self {
        SCREEN_ADJUSTING.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for BrightnessAdjustmentGuard {
    fn drop(&mut self) {
        SCREEN_ADJUSTING.fetch_sub(1, Ordering::SeqCst);
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
        .args([
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_adjustment_guard_restores_adjusting_counter() {
        let start = SCREEN_ADJUSTING.load(Ordering::SeqCst);
        {
            let _guard = BrightnessAdjustmentGuard::new();
            assert_eq!(SCREEN_ADJUSTING.load(Ordering::SeqCst), start + 1);
        }
        assert_eq!(SCREEN_ADJUSTING.load(Ordering::SeqCst), start);
    }

    #[test]
    fn join_worker_drains_brightness_thread_handle() {
        let mut worker = BrightnessWorker {
            tx: None,
            worker: Some(thread::spawn(|| {})),
        };

        worker.join_worker();

        assert!(worker.worker.is_none());
    }
}
