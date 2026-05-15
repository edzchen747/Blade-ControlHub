use brightness::Brightness;
use futures::stream::StreamExt;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::ui::app::app;
use crate::ui::app_events::OsdEvent;

pub static SCREEN_TARGET_LVL: AtomicU8 = AtomicU8::new(100);
pub static SCREEN_ADJUSTING: AtomicUsize = AtomicUsize::new(0);

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
        println!("Trying to set screen brightness: {}", discrete_lvl);
        SCREEN_TARGET_LVL.store(discrete_lvl, Ordering::SeqCst);
        let _ = self.tx.send(());
    }

    pub fn adjust_screen_brightness(&self, change: i8) {
        let current = { SCREEN_TARGET_LVL.load(Ordering::SeqCst) } as i8;
        let new_val = (current + change).clamp(0, 100) as u8;
        self.set_screen_brightness(new_val);
    }

    fn worker_loop(rx: Receiver<()>) {
        let mut monitor = pollster::block_on(async {
            let mut devices = brightness::brightness_devices();
            if let Some(Ok(dev)) = devices.next().await {
                return Some(dev); // Return the first working device found
            }
            println!("Fatal internal error: no monitor device found");
            None
        });
        let mut last_processed_lvl: u8 = 101;
        // Block on the channel waiting for "pokes"
        while rx.recv().is_ok() {
            while rx.try_recv().is_ok() {} // drain queue in channel to prevent extra updates
            // Get the latest target
            let target = SCREEN_TARGET_LVL.load(Ordering::SeqCst);

            // skip if target has not changed
            if target == last_processed_lvl {
                println!("skipped as last brightness was: {}", last_processed_lvl);
                continue;
            }
            SCREEN_ADJUSTING.fetch_add(1, Ordering::SeqCst);
            // Run the async logic to update the first successful device
            pollster::block_on(async {
                let mut success = false;

                if let Some(ref mut dev) = monitor {
                    if dev.set(target as u32).await.is_ok() {
                        success = true;
                        last_processed_lvl = target;
                    }
                }

                if !success {
                    eprintln!("Worker: Failed to update any brightness devices.");
                };
                SCREEN_ADJUSTING.fetch_sub(1, Ordering::SeqCst);
                println!("Set brightness: {}", target);
            });
        }
    }
}
