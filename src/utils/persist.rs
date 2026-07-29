use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tracing::{debug, error, warn};

// Module-local - accessed only within this module via enable()/disable() methods
static PERSIST_ENABLED: AtomicBool = AtomicBool::new(true);

/// A debounced file writer that batches rapid writes into a single disk commit.
///
/// Sends to `PersistBuffer` are buffered and only flushed to disk after a 2-second
/// quiet period, preventing excessive I/O during bursts of configuration changes.
pub struct PersistBuffer {
    tx: Option<Sender<String>>,
    worker: Option<JoinHandle<()>>,
}

impl PersistBuffer {
    pub fn new(path: String) -> Self {
        let (tx, rx) = mpsc::channel::<String>();

        let worker = match thread::Builder::new()
            .name("blade-persist-buffer".to_string())
            .spawn(move || Self::worker_loop(path, rx))
        {
            Ok(worker) => Some(worker),
            Err(error) => {
                error!(%error, "Failed to start persist buffer thread");
                None
            }
        };

        Self {
            tx: Some(tx),
            worker,
        }
    }

    pub fn write(&self, content: String) {
        let Some(tx) = &self.tx else {
            warn!("Persist buffer write requested after sender was closed");
            return;
        };

        if let Err(error) = tx.send(content) {
            warn!(
                ?error,
                "Persist buffer worker is unavailable; dropping config write"
            );
        }
    }

    fn perform_commit(path: &str, content: &str) {
        match File::create(path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(content.as_bytes()) {
                    error!(path = path, %error, "Failed to write config content");
                    return;
                }
                if let Err(error) = file.flush() {
                    error!(path = path, %error, "Failed to flush config file");
                    return;
                }
                debug!(path = path, "Config persisted to disk");
            }
            Err(e) => error!(path = path, error = %e, "Failed to write config"),
        }
    }

    fn worker_loop(path: String, rx: mpsc::Receiver<String>) {
        let mut pending_content: Option<String> = None;
        let mut flush_at: Option<Instant> = None;
        let mut last_written_content = String::new();

        loop {
            let timeout = if let Some(target) = flush_at {
                target.saturating_duration_since(Instant::now())
            } else {
                Duration::MAX
            };
            // New data sent to be persisted
            let recv = rx.recv_timeout(timeout);
            if !PERSIST_ENABLED.load(Ordering::SeqCst) {
                continue;
            }
            match recv {
                Ok(content) => {
                    if flush_at.is_none() {
                        // Set timer for a "debounced" write (2 seconds)
                        flush_at = Some(Instant::now() + Duration::from_secs(2));
                    }
                    pending_content = Some(content);
                }

                // Timer expired: Time to save to disk
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(content) = pending_content.take() {
                        // Only write if it's actually different from what is on disk
                        if content != last_written_content {
                            Self::perform_commit(&path, &content);
                            last_written_content = content;
                        }
                    }
                    flush_at = None;
                }

                // Final emergency save
                Err(RecvTimeoutError::Disconnected) => {
                    if let Some(content) = pending_content.take()
                        && content != last_written_content
                    {
                        Self::perform_commit(&path, &content);
                    }
                    break;
                }
            }
        }
    }

    pub fn enable() {
        PERSIST_ENABLED.store(true, Ordering::SeqCst);
    }

    pub fn disable() {
        PERSIST_ENABLED.store(false, Ordering::SeqCst);
    }
}

impl Drop for PersistBuffer {
    fn drop(&mut self) {
        drop(self.tx.take());

        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            warn!("Persist buffer worker panicked during shutdown");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU64;

    static TEST_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_config_path(test_name: &str) -> PathBuf {
        let id = TEST_FILE_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "blade-controlhub-{test_name}-{}-{id}.json",
            std::process::id()
        ))
    }

    #[test]
    fn drop_flushes_pending_content_without_waiting_for_debounce() {
        PersistBuffer::enable();
        let path = temp_config_path("drop-flushes-pending-content");
        let buffer = PersistBuffer::new(path.to_string_lossy().into_owned());

        buffer.write("{\"screen_lvl\":50}".to_string());
        drop(buffer);

        let saved = fs::read_to_string(&path).expect("pending content must be flushed on drop");
        let _ = fs::remove_file(path);

        assert_eq!(saved, "{\"screen_lvl\":50}");
    }

    #[test]
    fn drop_flushes_latest_pending_content_only() {
        PersistBuffer::enable();
        let path = temp_config_path("drop-flushes-latest-pending-content");
        let buffer = PersistBuffer::new(path.to_string_lossy().into_owned());

        buffer.write("old".to_string());
        buffer.write("new".to_string());
        drop(buffer);

        let saved = fs::read_to_string(&path).expect("latest content must be flushed on drop");
        let _ = fs::remove_file(path);

        assert_eq!(saved, "new");
    }
}
