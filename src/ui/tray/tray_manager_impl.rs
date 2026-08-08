impl TrayManager {
    /// Initializes and builds the system tray manager, context menu,
    /// mouse click listeners, and zero-idle icon updater thread.
    pub fn start() {
        join_finished_tray_threads();

        // Prevent double-initialization if called multiple times
        if TRAY_INITIALIZED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        TRAY_SHUTDOWN.store(false, Ordering::SeqCst);

        let (tx, rx): (Sender<PerfMode>, Receiver<PerfMode>) = channel();
        *tray_update_sender() = Some(tx);

        Self::setup_menu_event_handler();
        Self::spawn_tray_click_listener();

        match thread::Builder::new()
            .name("blade-tray-icon".to_string())
            .spawn(move || Self::run_tray_icon_loop(rx))
        {
            Ok(handle) => {
                *tray_icon_thread() = Some(handle);
            }
            Err(error) => {
                warn!(%error, "Failed to start tray icon thread");
                reset_tray_state();
                join_tray_click_thread();
            }
        }
    }

    pub fn shutdown() {
        TRAY_SHUTDOWN.store(true, Ordering::SeqCst);
        wake_tray_message_loop();
        *tray_update_sender() = None;
        join_tray_threads();
    }

    // ── Menu Construction & Handling ─────────────────────────────────────────

    fn build_tray_menu() -> Menu {
        let tray_menu = Menu::new();

        let quit_item = MenuItem::with_id("quit", "Quit", true, None);
        let restart_item = MenuItem::with_id("restart", "Restart", true, None);
        let settings_item = MenuItem::with_id("settings_window", "Settings", true, None);
        let close_gpu_apps_item =
            MenuItem::with_id("close_gpu_apps", "Close apps running on dGPU", true, None);

        let _ = tray_menu.append(&close_gpu_apps_item);
        let _ = tray_menu.append(&settings_item);
        let _ = tray_menu.append(&restart_item);
        let _ = tray_menu.append(&quit_item);

        tray_menu
    }

    fn setup_menu_event_handler() {
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id.0.as_str() {
            "quit" => {
                app(AppEvent::Shutdown);
            }
            "restart" => {
                app(AppEvent::Restart(0));
            }
            "settings_window" => {
                app(AppEvent::OpenSettings);
            }
            "close_gpu_apps" => {
                cycle_gpu();
            }
            _ => {}
        }));
    }

    // ── Tray Click Listener ──────────────────────────────────────────────────

    fn spawn_tray_click_listener() {
        match thread::Builder::new()
            .name("blade-tray-click-listener".to_string())
            .spawn(move || {
                let receiver = TrayIconEvent::receiver();
                while !TRAY_SHUTDOWN.load(Ordering::SeqCst) {
                    let Ok(event) = receiver.recv_timeout(Duration::from_millis(250)) else {
                        continue;
                    };

                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        debug!("Tray icon clicked, toggling settings");
                        app(AppEvent::ToggleSettings);
                    }
                }
            }) {
            Ok(handle) => {
                *tray_click_thread() = Some(handle);
            }
            Err(error) => {
                warn!(%error, "Failed to start tray click listener thread");
            }
        }
    }

    fn run_tray_icon_loop(rx: Receiver<PerfMode>) {
        TRAY_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
        let Some(icon) = Self::load_tray_icon(DEFAULT_ICON_COLOR) else {
            warn!("Tray initialization aborted because the tray icon could not be created");
            reset_tray_state();
            return;
        };
        let tray_menu = Self::build_tray_menu();

        let mut tray_icon = match TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip(APP_TOOLTIP)
            .with_icon(icon)
            .build()
        {
            Ok(icon) => icon,
            Err(error) => {
                warn!(?error, "Failed to build Windows tray icon");
                reset_tray_state();
                return;
            }
        };

        Self::run_tray_message_loop(&rx, &mut tray_icon);
        reset_tray_state();
    }

    fn run_tray_message_loop(rx: &Receiver<PerfMode>, tray_icon: &mut TrayIcon) {
        let mut msg = MSG::default();
        while !TRAY_SHUTDOWN.load(Ordering::SeqCst) {
            if Self::drain_perf_mode_updates(rx, tray_icon) {
                Self::pump_pending_messages(&mut msg);
            }

            unsafe {
                MsgWaitForMultipleObjects(None, false, 250, QS_ALLINPUT);
            }

            if Self::pump_pending_messages(&mut msg) {
                break;
            }
        }
    }

    fn drain_perf_mode_updates(rx: &Receiver<PerfMode>, tray_icon: &mut TrayIcon) -> bool {
        let mut updated = false;
        while let Ok(mode) = rx.try_recv() {
            Self::set_perf_mode_icon_tooltip(tray_icon, mode);
            updated = true;
        }
        updated
    }

    fn pump_pending_messages(msg: &mut MSG) -> bool {
        let mut quit = false;
        unsafe {
            while PeekMessageW(msg, None, 0, 0, PM_REMOVE).into() {
                if msg.message == WM_QUIT {
                    quit = true;
                    break;
                }
                let _ = TranslateMessage(msg);
                DispatchMessageW(msg);
            }
        }
        quit
    }

    // ── Icon Rasterization & Theme Coloring ──────────────────────────────────

    fn load_tray_icon(hex_color: &str) -> Option<Icon> {
        let Some(mut pixmap) = tiny_skia::Pixmap::new(TRAY_ICON_SIZE, TRAY_ICON_SIZE) else {
            warn!("Failed to allocate tray icon pixmap");
            return None;
        };

        let colored_svg = include_str!("../../../assets/icon.svg")
            .replace("#FFFFFF", hex_color)
            .replace("#ffffff", &hex_color.to_lowercase());

        let opt = usvg::Options::default();
        let tree = match usvg::Tree::from_str(&colored_svg, &opt) {
            Ok(tree) => tree,
            Err(error) => {
                warn!(?error, "Failed to parse tray icon SVG");
                return None;
            }
        };

        let svg_size = tree.size();
        let base_scale = (TRAY_ICON_SIZE as f32 / svg_size.width())
            .min(TRAY_ICON_SIZE as f32 / svg_size.height());
        let final_scale = base_scale * TRAY_ICON_SCALE_FACTOR;

        let tx = (TRAY_ICON_SIZE as f32 - (svg_size.width() * final_scale)) / 2.0;
        let ty = (TRAY_ICON_SIZE as f32 - (svg_size.height() * final_scale)) / 2.0;

        let transform =
            tiny_skia::Transform::from_scale(final_scale, final_scale).post_translate(tx, ty);

        resvg::render(&tree, transform, &mut pixmap.as_mut());

        let rgba = pixmap.take();
        match Icon::from_rgba(rgba, TRAY_ICON_SIZE, TRAY_ICON_SIZE) {
            Ok(icon) => Some(icon),
            Err(error) => {
                warn!(?error, "Failed to create tray icon from RGBA pixels");
                None
            }
        }
    }

    fn set_perf_mode_icon_tooltip(tray_icon: &mut TrayIcon, perf_mode: PerfMode) {
        debug!(mode = ?perf_mode, "Switching tray icon colour");
        let hex = perf_mode_hex_color(perf_mode);
        if let Some(new_icon) = Self::load_tray_icon(hex) {
            let _ = tray_icon.set_icon(Some(new_icon));
            let _ =
                tray_icon.set_tooltip(Some(format!("{} - {}", APP_TOOLTIP, perf_mode)));
        }
    }

    pub fn set_tray_icon(mode: PerfMode) {
        if !TRAY_INITIALIZED.load(Ordering::SeqCst) {
            Self::start();
        }

        if let Some(sender) = tray_update_sender().as_ref() {
            let _ = sender.send(mode);
            wake_tray_message_loop();
        }
    }
}

