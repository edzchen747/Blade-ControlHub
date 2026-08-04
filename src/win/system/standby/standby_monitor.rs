pub struct StandbyMonitor {}

impl StandbyMonitor {
    pub fn start() {
        join_finished_standby_monitor_thread();
        join_finished_standby_watchdog_thread();

        if STANDBY_MONITOR_RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }

        match thread::Builder::new()
            .name("blade-standby-monitor".to_string())
            .spawn(run_standby_monitor_loop)
        {
            Ok(handle) => {
                *standby_monitor_thread() = Some(handle);
            }
            Err(error) => {
                STANDBY_MONITOR_RUNNING.store(false, Ordering::SeqCst);
                error!(%error, "Failed to start standby monitor thread");
            }
        }
    }

    pub fn stop() {
        STANDBY_MONITOR_RUNNING.store(false, Ordering::SeqCst);
        standby_watchdog_wake().1.notify_all();
        wake_standby_monitor(WM_STANDBY_STOP);
        join_standby_monitor_thread();
        join_standby_watchdog_thread();
    }
}

fn standby_monitor_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    STANDBY_MONITOR_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_standby_monitor_thread() {
    let should_join = standby_monitor_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);

    if should_join {
        join_standby_monitor_thread();
    }
}

fn standby_watchdog_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    STANDBY_WATCHDOG_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_standby_watchdog_thread() {
    let should_join = standby_watchdog_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);

    if should_join {
        join_standby_watchdog_thread();
    }
}

fn join_standby_monitor_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = standby_monitor_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current standby monitor thread during shutdown");
        *standby_monitor_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("Standby monitor thread panicked during shutdown");
    }
}

fn join_standby_watchdog_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = standby_watchdog_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current standby watchdog thread during shutdown");
        *standby_watchdog_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("Standby watchdog thread panicked during shutdown");
    }
}

fn run_standby_monitor_loop() {
    let Some(window) = StandbyWindow::create() else {
        STANDBY_MONITOR_RUNNING.store(false, Ordering::SeqCst);
        return;
    };
    let hwnd = window.hwnd;
    MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

    let _subscription = StandbySubscriptions::register(hwnd);

    info!("Windows standby monitor started");
    start_standby_watchdog();

    let mut last_state = StandbyState::Normal;
    run_message_loop(&mut last_state);
    MAIN_HWND.store(0, Ordering::SeqCst);
    STANDBY_MONITOR_RUNNING.store(false, Ordering::SeqCst);
    standby_watchdog_wake().1.notify_all();
    join_standby_watchdog_thread();
}

fn wake_standby_monitor(message: u32) {
    let hwnd = HWND(MAIN_HWND.load(Ordering::SeqCst) as *mut _);
    if !hwnd.0.is_null() {
        let _ = unsafe { PostMessageW(hwnd, message, WPARAM(0), LPARAM(0)) };
    }
}

fn run_message_loop(last_state: &mut StandbyState) {
    unsafe {
        let mut msg = MSG::default();
        while STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst)
            && GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool()
        {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);

            match msg.message {
                WM_POWERBROADCAST => process_standby_change(last_state),
                WM_WTSSESSION_CHANGE => process_standby_change(last_state),
                WM_STANDBY_CHANGE => process_standby_change(last_state),
                WM_STANDBY_STOP => break,
                _ => {}
            }
        }
    }
}

fn start_standby_watchdog() {
    join_finished_standby_watchdog_thread();

    if standby_watchdog_thread().is_some() {
        warn!("Standby watchdog is already running");
        return;
    }

    match thread::Builder::new()
        .name("blade-standby-watchdog".to_string())
        .spawn(run_standby_watchdog_loop)
    {
        Ok(handle) => {
            *standby_watchdog_thread() = Some(handle);
        }
        Err(error) => {
            warn!(%error, "Failed to start standby watchdog thread");
        }
    }
}

fn run_standby_watchdog_loop() {
    let mut last_tick = SystemTime::now();
    while STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst) {
        wait_for_standby_watchdog(STANDBY_WATCHDOG_INTERVAL);
        if !STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst) {
            break;
        }

        let now = SystemTime::now();
        if let Ok(elapsed) = now.duration_since(last_tick)
            && resume_gap_detected(elapsed)
        {
            info!(
                elapsed_ms = elapsed.as_millis() as u64,
                "Standby watchdog detected resume after system sleep"
            );
            if update_standby_state_from_watchdog_resume() {
                wake_standby_monitor(WM_STANDBY_CHANGE);
            }
        }
        last_tick = now;
    }
}

fn standby_watchdog_wake() -> Arc<StandbyWatchdogWake> {
    STANDBY_WATCHDOG_WAKE
        .get_or_init(|| Arc::new((Mutex::new(()), Condvar::new())))
        .clone()
}

fn wait_for_standby_watchdog(duration: Duration) {
    let signal = standby_watchdog_wake();
    let (lock, cvar) = &*signal;
    let guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (_guard, _timeout) = cvar
        .wait_timeout_while(guard, duration, |_| {
            STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst)
        })
        .unwrap_or_else(|poisoned| poisoned.into_inner());
}

fn process_standby_change(last_state: &mut StandbyState) {
    if !STANDBY_MONITOR_RUNNING.load(Ordering::SeqCst) {
        return;
    }

    let mut lock = standby_state();
    if *lock == *last_state {
        return;
    }

    match *lock {
        StandbyState::Sleep => {
            run_sleep_cleanup("queued_power_event");
        }
        StandbyState::Wake => {
            SLEEP_CLEANUP_STARTED.store(false, Ordering::SeqCst);
            recover_from_system_wake("power_event", true);
            *lock = StandbyState::Normal;
        }
        StandbyState::Normal => {}
    };
    *last_state = *lock;
}

fn run_sleep_cleanup(source: &'static str) {
    if SLEEP_CLEANUP_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    match razer::device_handle::device().sleep() {
        Ok(_) => info!(
            source,
            "System entering sleep; executed hardware shutdown sequence"
        ),
        Err(error) => {
            warn!(%error, source, "System entering sleep; hardware shutdown sequence did not finish")
        }
    }
}

fn recover_from_system_wake(source: &'static str, refresh_display: bool) {
    match razer::device_handle::device().reinitialize() {
        Ok(device_pid) => {
            if let Err(error) = crate::win::input::reinitialize_keyboard_hooks(device_pid) {
                warn!(%error, source, "Failed to restart keyboard and HID listeners after system wake");
            }
        }
        Err(error) => {
            warn!(%error, source, "Failed to reopen Razer HID device after system wake");
        }
    }
    if refresh_display {
        GpuDisplayMonitor::trigger_display_change();
    }
    info!(source, "System waking from sleep; re-initialised hardware handles");
}

