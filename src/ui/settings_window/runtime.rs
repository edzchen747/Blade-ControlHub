fn try_load_settings_state() -> Option<SettingsState> {
    let started = Instant::now();
    debug!("Loading settings state from Blade ControlHub runtime");
    match client::get_settings_state() {
        Ok(state) => {
            info!(
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Loaded settings state from Blade ControlHub runtime"
            );
            Some(state)
        }
        Err(error) => {
            warn!(
                %error,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Failed to load settings state from Blade ControlHub runtime"
            );
            None
        }
    }
}

pub fn run() -> eframe::Result<()> {
    let process_started = Instant::now();
    if let Err(error) = set_cwd() {
        eprintln!("Failed to set settings window working directory: {error}");
    }
    init_log_file_writer_for_child("Start Settings Window Session");
    info!("Settings window process started");

    let initial_state = try_load_settings_state();
    info!(
        state_loaded = initial_state.is_some(),
        elapsed_ms = process_started.elapsed().as_millis() as u64,
        "Settings window initial state load completed"
    );

    let icon_started = Instant::now();
    let icon_data = Settings::load_settings_icon(
        initial_state
            .as_ref()
            .map(|state| state.theme_color)
            .unwrap_or(SETTINGS_LOADING_ICON_COLOR),
    );
    debug!(
        elapsed_ms = icon_started.elapsed().as_millis() as u64,
        "Loaded settings window icon"
    );
    let window_size = SETTINGS_WINDOW_SIZE;

    let mut native_options = eframe::NativeOptions {
        follow_system_theme: true,
        ..Default::default()
    };
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(SETTINGS_WINDOW_TITLE)
        .with_icon(icon_data)
        .with_inner_size(window_size)
        .with_min_inner_size(window_size)
        .with_max_inner_size(window_size)
        .with_resizable(false)
        .with_maximize_button(false);

    let monitor_started = Instant::now();
    if let Some(monitor) = pre_resolve_monitor_dimensions() {
        debug!(
            elapsed_ms = monitor_started.elapsed().as_millis() as u64,
            "Resolved settings window spawn position"
        );
        viewport = viewport.with_position(settings_spawn_position(monitor, window_size));
    } else {
        debug!(
            elapsed_ms = monitor_started.elapsed().as_millis() as u64,
            "Settings window spawn position was not pre-resolved"
        );
    }

    native_options.viewport = viewport;

    info!(
        elapsed_ms = process_started.elapsed().as_millis() as u64,
        "Starting settings native window"
    );
    let result = eframe::run_native(
        SETTINGS_WINDOW_TITLE,
        native_options,
        Box::new(move |cc| {
            let mut app = SettingsApp::new(initial_state);
            app.start_settings_update_worker(cc.egui_ctx.clone());
            Box::new(app)
        }),
    );

    match &result {
        Ok(()) => info!(
            elapsed_ms = process_started.elapsed().as_millis() as u64,
            "Settings native window exited"
        ),
        Err(error) => warn!(
            error = ?error,
            elapsed_ms = process_started.elapsed().as_millis() as u64,
            "Settings native window failed"
        ),
    }

    result
}

