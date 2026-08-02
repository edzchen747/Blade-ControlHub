use crate::{
    core::shared_state::{SCREEN_ADJUSTING, SCREEN_TARGET_LVL},
    razer::device_handle::{DeviceHandle, device},
    win::{
        audio::{self, AudioType},
        display::screen_query::get_screen_brightness,
    },
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::{debug, warn};

static EXTERNAL_MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);
static EXTERNAL_MONITOR_WAKE: OnceLock<Arc<ExternalMonitorWake>> = OnceLock::new();
static EXTERNAL_MONITOR_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

type ExternalMonitorWake = (Mutex<()>, Condvar);

pub struct ExternalChangeMonitor {
    fast_interval: Duration,
    slow_interval: Duration,
    mic_muted: bool,
    speakers_muted: bool,
    screen_brightness: u8,
    backend: Box<dyn ExternalMonitorBackend>,
}

impl ExternalChangeMonitor {
    fn new(
        fast_interval: Duration,
        slow_interval: Duration,
        backend: Box<dyn ExternalMonitorBackend>,
    ) -> Self {
        let mic_muted = backend.mic_muted();
        let speakers_muted = backend.speakers_muted();
        let screen_brightness = backend.screen_brightness();

        Self {
            fast_interval,
            slow_interval,
            mic_muted,
            speakers_muted,
            screen_brightness,
            backend,
        }
    }

    pub fn start() {
        join_finished_external_monitor_thread();

        if EXTERNAL_MONITOR_RUNNING.swap(true, Ordering::SeqCst) {
            warn!("External change monitor is already running");
            return;
        }

        match thread::Builder::new()
            .name("blade-external-change-monitor".to_string())
            .spawn(|| {
                Self::new(
                    Duration::from_millis(100),
                    Duration::from_millis(1000),
                    Box::new(ProductionExternalMonitorBackend::new(device())),
                )
                .run_loop();
            }) {
            Ok(handle) => {
                *external_monitor_thread() = Some(handle);
            }
            Err(error) => {
                EXTERNAL_MONITOR_RUNNING.store(false, Ordering::SeqCst);
                warn!(%error, "Failed to start external change monitor thread");
            }
        }
    }

    pub fn stop() {
        EXTERNAL_MONITOR_RUNNING.store(false, Ordering::SeqCst);
        external_monitor_wake().1.notify_all();
        join_external_monitor_thread();
    }

    fn run_loop(&mut self) {
        let mut interval = Duration::ZERO;
        while EXTERNAL_MONITOR_RUNNING.load(Ordering::SeqCst) {
            interval = self.poll_once(interval);
            wait_for_external_monitor(self.fast_interval);
        }
    }

    fn poll_once(&mut self, mut interval: Duration) -> Duration {
        let curr_mic_muted = self.backend.mic_muted();
        if curr_mic_muted != self.mic_muted {
            self.backend.set_mic_mute_indicator(curr_mic_muted);
        };
        self.mic_muted = curr_mic_muted;

        let curr_speakers_muted = self.backend.speakers_muted();
        if curr_speakers_muted != self.speakers_muted {
            self.backend
                .set_speakers_mute_indicator(curr_speakers_muted);
        };
        self.speakers_muted = curr_speakers_muted;

        interval += self.fast_interval;

        if interval >= self.slow_interval && SCREEN_ADJUSTING.load(Ordering::SeqCst) == 0 {
            interval = Duration::ZERO;
            let curr_screen_brightness = self.backend.screen_brightness();
            if self.screen_brightness != curr_screen_brightness {
                SCREEN_TARGET_LVL.store(curr_screen_brightness, Ordering::SeqCst);
                self.backend.persist_config();
                self.screen_brightness = curr_screen_brightness;
                debug!(
                    brightness = curr_screen_brightness,
                    "External brightness change detected"
                );
            }
        }

        interval
    }
}

fn external_monitor_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    EXTERNAL_MONITOR_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_external_monitor_thread() {
    let should_join = external_monitor_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);

    if should_join {
        join_external_monitor_thread();
    }
}

fn join_external_monitor_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = external_monitor_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current external change monitor thread during shutdown");
        *external_monitor_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("External change monitor thread panicked during shutdown");
    }
}

