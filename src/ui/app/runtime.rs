pub fn init() {
    let _ = APP_CONTEXT.get_or_init(|| AppContext {
        core: Arc::new(Mutex::new(AppCore::new())),
        device: device(),
    });
    crate::ipc::server::start(device());
    let _ = shutdown_signal();
}

pub fn run() {
    init();
    let shutdown_signal = shutdown_signal();
    wait_for_shutdown(&shutdown_signal);
}

fn active_ui_process() -> MutexGuard<'static, Option<Child>> {
    ACTIVE_UI_PROCESS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn core(ctx: &AppContext) -> MutexGuard<'_, AppCore> {
    ctx.core
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn shutdown_signal() -> Arc<ShutdownSignal> {
    SHUTDOWN_SIGNAL
        .get_or_init(|| Arc::new((Mutex::new(false), Condvar::new())))
        .clone()
}

fn signal_shutdown() {
    let signal = shutdown_signal();
    let (lock, cvar) = &*signal;
    let mut should_shutdown = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *should_shutdown = true;
    cvar.notify_all();
}

fn wait_for_shutdown(signal: &ShutdownSignal) {
    let (lock, cvar) = signal;
    let mut should_shutdown = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    while !*should_shutdown {
        should_shutdown = cvar
            .wait(should_shutdown)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    }
}

fn shutdown_runtime(ctx: &AppContext, reason: &str) {
    TrayManager::shutdown();
    ExternalChangeMonitor::stop();
    OsdController::stop();
    PowerMonitor::stop();
    StandbyMonitor::stop();
    GpuDisplayMonitor::stop();
    stop_keyboard_hooks();
    crate::ipc::server::stop();
    stop_device_channel_monitor();
    if let Err(error) = ctx.device.shutdown() {
        warn!(%error, reason, "Device shutdown cleanup did not complete");
    }
    terminate_active_settings_process(reason);
    core(ctx).running = false;
    signal_shutdown();
}

fn open_or_toggle_settings(ctx: &AppContext) {
    let total_started = Instant::now();
    let state_started = Instant::now();
    debug!("Loading settings state before launching settings window");
    let settings_state = match ctx.device.get_settings_state() {
        Ok(state) => {
            info!(
                elapsed_ms = state_started.elapsed().as_millis() as u64,
                "Loaded settings state before launching settings window"
            );
            state
        }
        Err(error) => {
            warn!(
                %error,
                elapsed_ms = state_started.elapsed().as_millis() as u64,
                "Failed to load settings state before launching settings window; using defaults"
            );
            Default::default()
        }
    };

    {
        let core = core(ctx);
        core.settings.show(settings_state);
    }

    let mut active_ui_process = active_ui_process();
    if close_running_settings_process_if_present(&mut active_ui_process) {
        info!(
            elapsed_ms = total_started.elapsed().as_millis() as u64,
            "Closed running settings window process"
        );
        return;
    }

    let settings_exe = match settings_executable_path() {
        Ok(path) => path,
        Err(error) => {
            warn!(%error, "Failed to resolve settings executable path");
            return;
        }
    };

    let spawn_started = Instant::now();
    info!(
        path = ?settings_exe,
        elapsed_ms = total_started.elapsed().as_millis() as u64,
        "Launching settings window process"
    );
    let mut settings_command = Command::new(&settings_exe);
    settings_command.args(settings_process_args());

    match settings_command.spawn() {
        Ok(child) => {
            info!(
                pid = child.id(),
                spawn_elapsed_ms = spawn_started.elapsed().as_millis() as u64,
                total_elapsed_ms = total_started.elapsed().as_millis() as u64,
                "Settings window process spawned"
            );
            *active_ui_process = Some(child);
        }
        Err(error) => warn!(
            %error,
            path = ?settings_exe,
            spawn_elapsed_ms = spawn_started.elapsed().as_millis() as u64,
            total_elapsed_ms = total_started.elapsed().as_millis() as u64,
            "Failed to launch settings window"
        ),
    }
}

fn close_running_settings_process_if_present(active_ui_process: &mut Option<Child>) -> bool {
    let Some(child) = active_ui_process.as_mut() else {
        return false;
    };

    match child_state(child) {
        ChildState::Running => {
            terminate_child(child, "settings toggle");
            *active_ui_process = None;
            true
        }
        ChildState::Exited(status) => {
            warn!(?status, "Settings process had already exited");
            *active_ui_process = None;
            false
        }
        ChildState::Unknown => {
            *active_ui_process = None;
            false
        }
    }
}

fn terminate_active_settings_process(reason: &str) {
    let mut active_ui_process = active_ui_process();
    let Some(mut child) = active_ui_process.take() else {
        return;
    };

    if matches!(child_state(&mut child), ChildState::Running) {
        terminate_child(&mut child, reason);
    }
}

fn terminate_child(child: &mut Child, reason: &str) {
    match child.kill() {
        Ok(()) => set_settings_window_state(false, false),
        Err(error) => warn!(%error, reason, "Failed to terminate settings process"),
    }
}

