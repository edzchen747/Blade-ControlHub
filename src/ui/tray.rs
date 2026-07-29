use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use resvg::{tiny_skia, usvg};
use tracing::debug;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, QS_ALLINPUT, TranslateMessage,
};

use crate::razer::enums::PerfMode;
use crate::ui::app::app;
use crate::ui::app_events::AppEvent;
use crate::ui::theme::{APP_TOOLTIP, DEFAULT_ICON_COLOR, TRAY_ICON_SCALE_FACTOR, TRAY_ICON_SIZE};
use crate::utils::reload::restart_app;
use crate::win::system::cli_utils::cycle_gpu;
use crate::win::system::startup::Startup;

// ── Globals & State ──────────────────────────────────────────────────────────

static STARTUP_STATE: AtomicBool = AtomicBool::new(false);
static TRAY_INITIALIZED: AtomicBool = AtomicBool::new(false);
static TRAY_UPDATE_SENDER: std::sync::OnceLock<Sender<PerfMode>> = std::sync::OnceLock::new();

// ── Tray Manager Struct ──────────────────────────────────────────────────────

pub struct TrayManager {
    pub tray_icon: TrayIcon,
}

impl TrayManager {
    /// Initializes and builds the system tray manager, context menu,
    /// mouse click listeners, and zero-idle icon updater thread.
    pub fn new() {
        // Prevent double-initialization if called multiple times
        if TRAY_INITIALIZED.load(Ordering::SeqCst) {
            return;
        }
        TRAY_INITIALIZED.store(true, Ordering::SeqCst);

        let (tx, rx): (Sender<PerfMode>, Receiver<PerfMode>) = channel();
        let _ = TRAY_UPDATE_SENDER.set(tx);

        Self::setup_menu_event_handler();
        Self::spawn_tray_click_listener();

        std::thread::spawn(move || {
            let icon = Self::load_tray_icon(DEFAULT_ICON_COLOR);
            let tray_menu = Self::build_tray_menu();

            let mut tray_icon = TrayIconBuilder::new()
                .with_menu(Box::new(tray_menu))
                .with_tooltip(APP_TOOLTIP)
                .with_icon(icon)
                .build()
                .unwrap();

            let mut msg = MSG::default();
            unsafe {
                loop {
                    // Non-blocking drain of pending performance mode changes
                    let mut updated = false;
                    while let Ok(mode) = rx.try_recv() {
                        Self::set_perf_mode_icon(&mut tray_icon, mode);
                        updated = true;
                    }

                    if updated {
                        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                            if msg.message == 0x0012 {
                                break;
                            }
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }

                    windows::Win32::UI::WindowsAndMessaging::MsgWaitForMultipleObjects(
                        None,
                        false,
                        u32::MAX,
                        QS_ALLINPUT,
                    );

                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                        if msg.message == 0x0012 {
                            // WM_QUIT
                            break;
                        }
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            }
        });
    }

    // ── Menu Construction & Handling ─────────────────────────────────────────

    fn build_tray_menu() -> Menu {
        let tray_menu = Menu::new();

        let quit_item = MenuItem::with_id("quit", "Quit", true, None);
        let restart_item = MenuItem::with_id("restart", "Restart", true, None);
        let settings_item = MenuItem::with_id("settings_window", "Settings", true, None);
        let close_gpu_apps_item =
            MenuItem::with_id("close_gpu_apps", "Close apps running on dGPU", true, None);

        let startup_detected = Startup::is_registered();
        let startup_item = CheckMenuItem::with_id(
            "startup_toggle",
            "Start with Windows",
            true,
            startup_detected,
            None,
        );

        STARTUP_STATE.store(startup_detected, Ordering::SeqCst);

        let _ = tray_menu.append(&quit_item);
        let _ = tray_menu.append(&restart_item);
        let _ = tray_menu.append(&settings_item);
        let _ = tray_menu.append(&startup_item);
        let _ = tray_menu.append(&close_gpu_apps_item);

        tray_menu
    }

    fn setup_menu_event_handler() {
        let startup_state = Arc::new(&STARTUP_STATE);

        MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id.0.as_str() {
            "quit" => {
                app(AppEvent::Shutdown);
            }
            "restart" => {
                restart_app(0);
            }
            "settings_window" => {
                app(AppEvent::OpenSettings);
            }
            "startup_toggle" => {
                let is_checked = !startup_state.load(Ordering::SeqCst);
                startup_state.store(is_checked, Ordering::SeqCst);
                if is_checked && !Startup::is_registered() {
                    Startup::register();
                } else if !is_checked && Startup::is_registered() {
                    Startup::unregister();
                }
            }
            "close_gpu_apps" => {
                cycle_gpu();
            }
            _ => {}
        }));
    }

    // ── Tray Click Listener ──────────────────────────────────────────────────

    fn spawn_tray_click_listener() {
        thread::spawn(move || {
            let receiver = TrayIconEvent::receiver();
            loop {
                if let Ok(event) = receiver.recv() {
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
            }
        });
    }

    // ── Icon Rasterization & Theme Coloring ──────────────────────────────────

    fn load_tray_icon(hex_color: &str) -> Icon {
        let mut pixmap = tiny_skia::Pixmap::new(TRAY_ICON_SIZE, TRAY_ICON_SIZE).unwrap();

        let coloured_svg = include_str!("../../assets/icon.svg")
            .replace("#FFFFFF", hex_color)
            .replace("#ffffff", &hex_color.to_lowercase());

        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_str(&coloured_svg, &opt).expect("Failed to parse SVG");

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
        Icon::from_rgba(rgba, TRAY_ICON_SIZE, TRAY_ICON_SIZE).expect("Failed to create tray icon")
    }

    fn set_perf_mode_icon(tray_icon: &mut TrayIcon, perf_mode: PerfMode) {
        debug!(mode = ?perf_mode, "Switching tray icon colour");
        let hex = Self::perf_mode_color(perf_mode);
        let new_icon = Self::load_tray_icon(hex);
        let _ = tray_icon.set_icon(Some(new_icon));

        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
            let current_thread_id = windows::Win32::System::Threading::GetCurrentThreadId();
            let _ = PostThreadMessageW(
                current_thread_id,
                windows::Win32::UI::WindowsAndMessaging::WM_NULL,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }

    fn perf_mode_color(mode: PerfMode) -> &'static str {
        match mode {
            PerfMode::BatterySaver => "#9BF542",
            PerfMode::Silent => "#00C853",
            PerfMode::Quiet => "#00E5FF",
            PerfMode::Balanced => "#FFD600",
            PerfMode::Performance => "#FF5D00",
            PerfMode::Turbo => "#D50000",
            PerfMode::Custom => "#A200FF",
            PerfMode::Unknown => DEFAULT_ICON_COLOR,
        }
    }

    pub fn set_tray_icon(mode: PerfMode) {
        if !TRAY_INITIALIZED.load(Ordering::SeqCst) {
            Self::new();
        }

        if let Some(sender) = TRAY_UPDATE_SENDER.get() {
            let _ = sender.send(mode);
        }
    }
}
