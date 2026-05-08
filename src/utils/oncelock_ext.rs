use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

// ── OnceLock Timeout Extension ──────────────────────────────────────────────

const ONCELOCK_TIMEOUT_MS: u64 = 500;
const ONCELOCK_POLL_INTERVAL_MS: u64 = 50;

pub trait OnceLockExt<T> {
    fn get_or_timeout(&self) -> Option<T>
    where
        T: Clone;
}

impl<T> OnceLockExt<T> for OnceLock<T> {
    /// Polls until the value is available or the timeout is reached.
    fn get_or_timeout(&self) -> Option<T>
    where
        T: Clone,
    {
        if let Some(v) = self.get() {
            return Some(v.clone());
        }

        let start = Instant::now();
        let timeout = Duration::from_millis(ONCELOCK_TIMEOUT_MS);
        let interval = Duration::from_millis(ONCELOCK_POLL_INTERVAL_MS);

        loop {
            thread::sleep(interval);
            if let Some(v) = self.get() {
                return Some(v.clone());
            }
            if start.elapsed() >= timeout {
                return None;
            }
        }
    }
}
