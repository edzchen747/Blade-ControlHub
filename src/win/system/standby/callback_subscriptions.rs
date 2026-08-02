struct CallbackStandbySubscription {
    handle: HPOWERNOTIFY,
    _params: Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>,
}

impl CallbackStandbySubscription {
    fn register() -> Option<Self> {
        let mut params = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(standby_callback),
            Context: null_mut(),
        });
        let mut handle = HPOWERNOTIFY::default();
        let result = unsafe {
            PowerRegisterSuspendResumeNotification(
                DEVICE_NOTIFY_CALLBACK,
                HANDLE((&mut *params) as *mut _ as *mut _),
                &mut handle.0 as *mut _ as *mut _,
            )
        };
        if result.is_err() {
            warn!(
                code = result.0,
                "Power suspend/resume callback registration failed"
            );
            return None;
        }

        Some(Self {
            handle,
            _params: params,
        })
    }
}

impl Drop for CallbackStandbySubscription {
    fn drop(&mut self) {
        let result = unsafe { PowerUnregisterSuspendResumeNotification(self.handle) };
        if result.is_err() {
            warn!(
                code = result.0,
                "Failed to unregister standby callback notification"
            );
        }
    }
}

struct StandbySubscriptions {
    _window: Option<WindowStandbySubscription>,
    _callback: Option<CallbackStandbySubscription>,
    _session: Option<SessionStandbySubscription>,
    _power_settings: Option<PowerSettingStandbySubscription>,
}

impl StandbySubscriptions {
    fn register(hwnd: HWND) -> Self {
        let window = WindowStandbySubscription::register(hwnd);
        if window.is_some() {
            info!("Registered standby window notification");
        } else {
            warn!("Standby window notification registration failed");
        }

        let callback = CallbackStandbySubscription::register();
        if callback.is_some() {
            info!("Registered standby callback notification");
        }

        let session = SessionStandbySubscription::register(hwnd);
        if session.is_some() {
            info!("Registered standby session notification");
        } else {
            warn!("Standby session notification registration failed");
        }

        let power_settings = PowerSettingStandbySubscription::register(hwnd);
        if power_settings.is_none() {
            warn!("No standby power setting notifications could be registered");
        }

        Self {
            _window: window,
            _callback: callback,
            _session: session,
            _power_settings: power_settings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn standby_event_mapping_updates_state_for_sleep_and_wake() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        assert!(update_standby_state_from_event(PBT_APMSUSPEND));
        assert_eq!(*standby_state(), StandbyState::Sleep);

        assert!(update_standby_state_from_event(PBT_APMRESUMEAUTOMATIC));
        assert_eq!(*standby_state(), StandbyState::Wake);
    }

    #[test]
    fn standby_event_mapping_updates_state_for_interactive_resume() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        assert!(update_standby_state_from_event(PBT_APMRESUMESUSPEND));
        assert_eq!(*standby_state(), StandbyState::Wake);
    }

    #[test]
    fn standby_event_mapping_ignores_unknown_event() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        *standby_state() = StandbyState::Normal;

        assert!(!update_standby_state_from_event(u32::MAX));
        assert_eq!(*standby_state(), StandbyState::Normal);
    }

    #[test]
    fn stop_clears_standby_monitor_running_flag() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        STANDBY_MONITOR_RUNNING.store(true, Ordering::SeqCst);
        MAIN_HWND.store(0, Ordering::SeqCst);

        StandbyMonitor::stop();

        assert!(!STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst));
        assert_eq!(MAIN_HWND.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn join_standby_monitor_thread_drains_handle() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *standby_monitor_thread() = Some(thread::spawn(|| {}));

        join_standby_monitor_thread();

        assert!(standby_monitor_thread().is_none());
    }

    #[test]
    fn resume_gap_detection_uses_threshold() {
        assert!(!resume_gap_detected(
            STANDBY_RESUME_GAP_THRESHOLD - Duration::from_millis(1)
        ));
        assert!(resume_gap_detected(STANDBY_RESUME_GAP_THRESHOLD));
    }

    #[test]
    fn watchdog_resume_sets_wake_from_normal() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        *standby_state() = StandbyState::Normal;

        assert!(update_standby_state_from_watchdog_resume());
        assert_eq!(*standby_state(), StandbyState::Wake);
    }

    #[test]
    fn session_unlock_maps_to_wake() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        *standby_state() = StandbyState::Normal;

        assert!(session_change_is_wake(WTS_SESSION_UNLOCK_EVENT));
        assert!(update_standby_state_from_session_resume());
        assert_eq!(*standby_state(), StandbyState::Wake);
    }

    #[test]
    fn display_power_settings_map_off_to_sleep_and_on_to_wake() {
        assert_eq!(
            display_power_setting_state(GUID_CONSOLE_DISPLAY_STATE, DISPLAY_POWER_OFF),
            Some(("console_display_state", StandbyState::Sleep))
        );
        assert_eq!(
            display_power_setting_state(GUID_SESSION_DISPLAY_STATUS, DISPLAY_POWER_ON),
            Some(("session_display_status", StandbyState::Wake))
        );
        assert_eq!(
            display_power_setting_state(GUID_MONITOR_POWER_ON, DISPLAY_POWER_OFF),
            Some(("monitor_power_on", StandbyState::Sleep))
        );
    }

    #[test]
    fn display_power_settings_ignore_dimmed_and_unknown_settings() {
        assert_eq!(
            display_power_setting_state(GUID_CONSOLE_DISPLAY_STATE, 2),
            None
        );
        assert_eq!(
            display_power_setting_state(windows::core::GUID::zeroed(), DISPLAY_POWER_OFF),
            None
        );
    }

    #[test]
    fn display_power_state_updates_standby_state() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        *standby_state() = StandbyState::Normal;

        assert!(update_standby_state_from_display_power(StandbyState::Sleep));
        assert_eq!(*standby_state(), StandbyState::Sleep);
        assert!(!update_standby_state_from_display_power(
            StandbyState::Sleep
        ));

        assert!(update_standby_state_from_display_power(StandbyState::Wake));
        assert_eq!(*standby_state(), StandbyState::Wake);
    }

    #[test]
    fn join_standby_watchdog_thread_drains_handle() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *standby_watchdog_thread() = Some(thread::spawn(|| {}));

        join_standby_watchdog_thread();

        assert!(standby_watchdog_thread().is_none());
    }
}
