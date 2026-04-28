use std::fs::File;
use std::io::Write;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

pub struct PersistBuffer {
    tx: Sender<String>,
}

impl PersistBuffer {
    pub fn new(path: String) -> Self {
        let (tx, rx) = mpsc::channel::<String>();

        thread::spawn(move || {
            let mut pending_content: Option<String> = None;
            let mut flush_at: Option<Instant> = None;
            let mut last_written_content = String::new(); // Change detection lives here

            loop {
                let timeout = if let Some(target) = flush_at {
                    target.saturating_duration_since(Instant::now())
                } else {
                    Duration::MAX
                };

                match rx.recv_timeout(timeout) {
                    // New data sent to be persisted
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
                        if let Some(content) = pending_content.take() {
                            if content != last_written_content {
                                Self::perform_commit(&path, &content);
                            }
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
                println!("Disk write completed: {}", path);
            }
            Err(e) => eprintln!("Failed to write config to {}: {}", path, e),
        }
    }
}
