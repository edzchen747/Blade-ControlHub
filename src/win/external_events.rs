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

trait ExternalMonitorBackend: Send {
    fn mic_muted(&self) -> bool;
    fn speakers_muted(&self) -> bool;
    fn screen_brightness(&self) -> u8;
    fn set_mic_mute_indicator(&self, muted: bool);
    fn set_speakers_mute_indicator(&self, muted: bool);
    fn persist_config(&self);
}

struct ProductionExternalMonitorBackend {
    device: DeviceHandle,
}

impl ProductionExternalMonitorBackend {
    fn new(device: DeviceHandle) -> Self {
        Self { device }
    }
}

impl ExternalMonitorBackend for ProductionExternalMonitorBackend {
    fn mic_muted(&self) -> bool {
        audio::is_audio_muted(AudioType::Mic)
    }

    fn speakers_muted(&self) -> bool {
        audio::is_audio_muted(AudioType::Speakers)
    }

    fn screen_brightness(&self) -> u8 {
        get_screen_brightness()
    }

    fn set_mic_mute_indicator(&self, muted: bool) {
        self.device.set_mic_mute_indicator(muted);
    }

    fn set_speakers_mute_indicator(&self, muted: bool) {
        self.device.set_speakers_mute_indicator(muted);
    }

    fn persist_config(&self) {
        self.device.persist_config();
    }
}

fn external_monitor_wake() -> Arc<ExternalMonitorWake> {
    EXTERNAL_MONITOR_WAKE
        .get_or_init(|| Arc::new((Mutex::new(()), Condvar::new())))
        .clone()
}

fn wait_for_external_monitor(duration: Duration) {
    let signal = external_monitor_wake();
    let (lock, cvar) = &*signal;
    let guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (_guard, _timeout) = cvar
        .wait_timeout_while(guard, duration, |_| {
            EXTERNAL_MONITOR_RUNNING.load(Ordering::SeqCst)
        })
        .unwrap_or_else(|poisoned| poisoned.into_inner());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicU8;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Default)]
    struct FakeExternalMonitorBackend {
        mic_muted: AtomicBool,
        speakers_muted: AtomicBool,
        screen_brightness: AtomicU8,
        mic_indicator: AtomicBool,
        speakers_indicator: AtomicBool,
        persist_called: AtomicBool,
    }

    impl ExternalMonitorBackend for FakeExternalMonitorBackend {
        fn mic_muted(&self) -> bool {
            self.mic_muted.load(Ordering::SeqCst)
        }

        fn speakers_muted(&self) -> bool {
            self.speakers_muted.load(Ordering::SeqCst)
        }

        fn screen_brightness(&self) -> u8 {
            self.screen_brightness.load(Ordering::SeqCst)
        }

        fn set_mic_mute_indicator(&self, muted: bool) {
            self.mic_indicator.store(muted, Ordering::SeqCst);
        }

        fn set_speakers_mute_indicator(&self, muted: bool) {
            self.speakers_indicator.store(muted, Ordering::SeqCst);
        }

        fn persist_config(&self) {
            self.persist_called.store(true, Ordering::SeqCst);
        }
    }

    impl ExternalMonitorBackend for Arc<FakeExternalMonitorBackend> {
        fn mic_muted(&self) -> bool {
            self.as_ref().mic_muted()
        }

        fn speakers_muted(&self) -> bool {
            self.as_ref().speakers_muted()
        }

        fn screen_brightness(&self) -> u8 {
            self.as_ref().screen_brightness()
        }

        fn set_mic_mute_indicator(&self, muted: bool) {
            self.as_ref().set_mic_mute_indicator(muted);
        }

        fn set_speakers_mute_indicator(&self, muted: bool) {
            self.as_ref().set_speakers_mute_indicator(muted);
        }

        fn persist_config(&self) {
            self.as_ref().persist_config();
        }
    }

    #[test]
    fn stop_clears_external_monitor_running_flag() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        EXTERNAL_MONITOR_RUNNING.store(true, Ordering::SeqCst);

        ExternalChangeMonitor::stop();

        assert!(!EXTERNAL_MONITOR_RUNNING.load(Ordering::SeqCst));
    }

    #[test]
    fn monitor_initializes_state_from_backend() {
        let backend = FakeExternalMonitorBackend::default();
        backend.mic_muted.store(true, Ordering::SeqCst);
        backend.screen_brightness.store(42, Ordering::SeqCst);

        let monitor = ExternalChangeMonitor::new(
            Duration::from_millis(1),
            Duration::from_millis(1),
            Box::new(backend),
        );

        assert!(monitor.mic_muted);
        assert_eq!(monitor.screen_brightness, 42);
    }

    #[test]
    fn poll_once_dispatches_changed_backend_state() {
        let backend = Arc::new(FakeExternalMonitorBackend::default());
        let mut monitor = ExternalChangeMonitor::new(
            Duration::from_millis(10),
            Duration::from_millis(10),
            Box::new(backend.clone()),
        );

        backend.mic_muted.store(true, Ordering::SeqCst);
        backend.speakers_muted.store(true, Ordering::SeqCst);
        backend.screen_brightness.store(80, Ordering::SeqCst);
        SCREEN_ADJUSTING.store(0, Ordering::SeqCst);
        SCREEN_TARGET_LVL.store(0, Ordering::SeqCst);

        let interval = monitor.poll_once(Duration::ZERO);

        assert_eq!(interval, Duration::ZERO);
        assert!(backend.mic_indicator.load(Ordering::SeqCst));
        assert!(backend.speakers_indicator.load(Ordering::SeqCst));
        assert!(backend.persist_called.load(Ordering::SeqCst));
        assert_eq!(SCREEN_TARGET_LVL.load(Ordering::SeqCst), 80);
    }

    #[test]
    fn join_external_monitor_thread_drains_handle() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *external_monitor_thread() = Some(thread::spawn(|| {}));

        join_external_monitor_thread();

        assert!(external_monitor_thread().is_none());
    }
}
