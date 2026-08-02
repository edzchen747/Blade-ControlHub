enum SettingsUpdateMessage {
    State(SettingsState),
}

fn spawn_settings_update_worker(
    initial_state_loaded: bool,
    shutdown: Arc<AtomicBool>,
    tx: Sender<SettingsUpdateMessage>,
    ctx: egui::Context,
) -> Option<thread::JoinHandle<()>> {
    match thread::Builder::new()
        .name("blade-settings-state-updates".to_string())
        .spawn(move || run_settings_update_worker(initial_state_loaded, shutdown, tx, ctx))
    {
        Ok(thread) => Some(thread),
        Err(error) => {
            warn!(%error, "Failed to spawn settings update worker");
            None
        }
    }
}

fn run_settings_update_worker(
    mut state_loaded: bool,
    shutdown: Arc<AtomicBool>,
    tx: Sender<SettingsUpdateMessage>,
    ctx: egui::Context,
) {
    let Some(listener) = SettingsUpdateListener::new() else {
        return;
    };
    let mut next_initial_retry = Instant::now();

    while !shutdown.load(Ordering::SeqCst) {
        let update_received = listener.wait(SETTINGS_UPDATE_WAIT_INTERVAL);
        if shutdown.load(Ordering::SeqCst) {
            break;
        }

        let retry_initial_state = !state_loaded && Instant::now() >= next_initial_retry;
        if !update_received && !retry_initial_state {
            continue;
        }
        if update_received {
            thread::sleep(SETTINGS_UPDATE_DEBOUNCE);
        }

        match try_load_settings_state() {
            Some(state) => {
                state_loaded = true;
                if tx.send(SettingsUpdateMessage::State(state)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
            None => {
                next_initial_retry = Instant::now() + SETTINGS_STATE_INITIAL_RETRY_INTERVAL;
            }
        }
    }
}

