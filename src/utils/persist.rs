use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use tracing::{debug, error};

// Module-local - accessed only within this module via enable()/disable() methods
static PERSIST_ENABLED: AtomicBool = AtomicBool::new(true);

/// A debounced file writer that batches rapid writes into a single disk commit.
///
/// Sends to `PersistBuffer` are buffered and only flushed to disk after a 2-second
/// quiet period, preventing excessive I/O during bursts of configuration changes.
pub struct PersistBuffer {
    tx: Sender<String>,
}

impl PersistBuffer {
    pub fn new(path: String) -> Self {
        let (tx, rx) = mpsc::channel::<String>();

        thread::spawn(move || {
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
        });

        Self { tx }
    }

    pub fn write(&self, content: String) {
        let _ = self.tx.send(content);
    }

    fn perform_commit(path: &str, content: &str) {
        match File::create(path) {
            Ok(mut file) => {
                let _ = file.write_all(content.as_bytes());
                let _ = file.flush();
                debug!(path = path, "Config persisted to disk");
            }
            Err(e) => error!(path = path, error = %e, "Failed to write config"),
        }
    }

    pub fn enable() {
        PERSIST_ENABLED.store(true, Ordering::SeqCst);
    }

    pub fn disable() {
        PERSIST_ENABLED.store(false, Ordering::SeqCst);
    }
}
