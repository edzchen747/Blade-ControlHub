fn tray_update_sender() -> MutexGuard<'static, Option<Sender<PerfMode>>> {
    TRAY_UPDATE_SENDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn tray_icon_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    TRAY_ICON_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn tray_click_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    TRAY_CLICK_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_tray_threads() {
    let should_join_icon = tray_icon_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);
    if should_join_icon {
        join_tray_icon_thread();
    }

    let should_join_click = tray_click_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);
    if should_join_click {
        join_tray_click_thread();
    }
}

fn join_tray_threads() {
    join_tray_icon_thread();
    join_tray_click_thread();
}

fn join_tray_icon_thread() {
    join_tray_thread(tray_icon_thread(), "tray icon");
}

fn join_tray_click_thread() {
    join_tray_thread(tray_click_thread(), "tray click listener");
}

fn join_tray_thread(
    mut thread_slot: MutexGuard<'static, Option<JoinHandle<()>>>,
    thread_name: &str,
) {
    let current_thread_id = thread::current().id();
    let Some(handle) = thread_slot.take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current {thread_name} thread during shutdown");
        *thread_slot = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("{thread_name} thread panicked during shutdown");
    }
}

fn reset_tray_state() {
    *tray_update_sender() = None;
    TRAY_SHUTDOWN.store(true, Ordering::SeqCst);
    TRAY_THREAD_ID.store(0, Ordering::SeqCst);
    TRAY_INITIALIZED.store(false, Ordering::SeqCst);
}

fn wake_tray_message_loop() {
    let thread_id = TRAY_THREAD_ID.load(Ordering::SeqCst);
    if thread_id != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_NULL, WPARAM(0), LPARAM(0));
        }
    }
}

