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
