use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tracing::{debug, error, warn};

static PERSIST_ENABLED: AtomicBool = AtomicBool::new(true);

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
        let target_path = Path::new(path);
        let temp_path = temp_commit_path(target_path);

        match File::create(&temp_path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(content.as_bytes()) {
                    error!(path = path, %error, "Failed to write config content");
                    let _ = fs::remove_file(&temp_path);
                    return;
                }
                if let Err(error) = file.flush() {
                    error!(path = path, %error, "Failed to flush config file");
                    let _ = fs::remove_file(&temp_path);
                    return;
                }
                if let Err(error) = file.sync_all() {
                    error!(path = path, %error, "Failed to sync config file");
                    let _ = fs::remove_file(&temp_path);
                    return;
                }
                drop(file);

                if let Err(error) = replace_file(&temp_path, target_path) {
                    error!(path = path, %error, "Failed to replace config file");
                    let _ = fs::remove_file(&temp_path);
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
            let recv = rx.recv_timeout(timeout);
            if !PERSIST_ENABLED.load(Ordering::SeqCst) {
                continue;
            }
            match recv {
                Ok(content) => {
                    if flush_at.is_none() {
                        flush_at = Some(Instant::now() + Duration::from_secs(2));
                    }
                    pending_content = Some(content);
                }

                Err(RecvTimeoutError::Timeout) => {
                    if let Some(content) = pending_content.take()
                        && content != last_written_content
                    {
                        Self::perform_commit(&path, &content);
                        last_written_content = content;
                    }
                    flush_at = None;
                }

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

fn temp_commit_path(target_path: &Path) -> PathBuf {
    let file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}.tmp"))
        .unwrap_or_else(|| "config.json.tmp".to_string());
    target_path.with_file_name(file_name)
}

#[cfg(target_os = "windows")]
fn replace_file(temp_path: &Path, target_path: &Path) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let temp = wide_null_path(temp_path);
    let target = wide_null_path(target_path);
    let replaced = unsafe {
        MoveFileExW(
            temp.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn wide_null_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn replace_file(temp_path: &Path, target_path: &Path) -> io::Result<()> {
    fs::rename(temp_path, target_path)
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

    #[test]
    fn commit_replaces_existing_file_content() {
        PersistBuffer::enable();
        let path = temp_config_path("commit-replaces-existing-file-content");
        fs::write(&path, "existing").expect("seed config must write");
        let buffer = PersistBuffer::new(path.to_string_lossy().into_owned());

        buffer.write("{\"screen_lvl\":80}".to_string());
        drop(buffer);

        let saved = fs::read_to_string(&path).expect("config must still exist after commit");
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(temp_commit_path(&path));

        assert_eq!(saved, "{\"screen_lvl\":80}");
    }
}
