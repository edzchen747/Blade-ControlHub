use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use resvg::{tiny_skia, usvg};
use tracing::{debug, warn};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, PostThreadMessageW,
    QS_ALLINPUT, TranslateMessage, WM_NULL, WM_QUIT,
};

use crate::razer::enums::PerfMode;
use crate::ui::app::app;
use crate::ui::app_events::AppEvent;
use crate::ui::theme::{APP_TOOLTIP, DEFAULT_ICON_COLOR, TRAY_ICON_SCALE_FACTOR, TRAY_ICON_SIZE};
use crate::win::system::cli_utils::cycle_gpu;
use crate::win::system::startup::Startup;

// ── Globals & State ──────────────────────────────────────────────────────────

static STARTUP_STATE: AtomicBool = AtomicBool::new(false);
static TRAY_INITIALIZED: AtomicBool = AtomicBool::new(false);
static TRAY_SHUTDOWN: AtomicBool = AtomicBool::new(false);
static TRAY_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static TRAY_UPDATE_SENDER: Mutex<Option<Sender<PerfMode>>> = Mutex::new(None);
static TRAY_ICON_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static TRAY_CLICK_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

// ── Tray Manager Struct ──────────────────────────────────────────────────────

pub struct TrayManager {
    pub tray_icon: TrayIcon,
}

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
            "startup_toggle" => {
                let is_checked = !STARTUP_STATE.load(Ordering::SeqCst);
                STARTUP_STATE.store(is_checked, Ordering::SeqCst);
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
            Self::set_perf_mode_icon(tray_icon, mode);
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

        let coloured_svg = include_str!("../../assets/icon.svg")
            .replace("#FFFFFF", hex_color)
            .replace("#ffffff", &hex_color.to_lowercase());

        let opt = usvg::Options::default();
        let tree = match usvg::Tree::from_str(&coloured_svg, &opt) {
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

    fn set_perf_mode_icon(tray_icon: &mut TrayIcon, perf_mode: PerfMode) {
        debug!(mode = ?perf_mode, "Switching tray icon colour");
        let hex = Self::perf_mode_color(perf_mode);
        if let Some(new_icon) = Self::load_tray_icon(hex) {
            let _ = tray_icon.set_icon(Some(new_icon));
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
            Self::start();
        }

        if let Some(sender) = tray_update_sender().as_ref() {
            let _ = sender.send(mode);
            wake_tray_message_loop();
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_lock() -> MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn perf_mode_color_maps_known_modes() {
        let _guard = test_lock();

        assert_eq!(TrayManager::perf_mode_color(PerfMode::Balanced), "#FFD600");
        assert_eq!(TrayManager::perf_mode_color(PerfMode::Turbo), "#D50000");
        assert_eq!(
            TrayManager::perf_mode_color(PerfMode::BatterySaver),
            "#9BF542"
        );
    }

    #[test]
    fn perf_mode_color_uses_default_for_unknown() {
        let _guard = test_lock();

        assert_eq!(
            TrayManager::perf_mode_color(PerfMode::Unknown),
            DEFAULT_ICON_COLOR
        );
    }

    #[test]
    fn tray_shutdown_sets_stop_flag_and_clears_sender() {
        let _guard = test_lock();

        let (tx, _rx) = channel();
        *tray_update_sender() = Some(tx);
        TRAY_SHUTDOWN.store(false, Ordering::SeqCst);

        TrayManager::shutdown();

        assert!(TRAY_SHUTDOWN.load(Ordering::SeqCst));
        assert!(tray_update_sender().is_none());
    }

    #[test]
    fn reset_tray_state_clears_thread_id_and_initialization() {
        let _guard = test_lock();

        TRAY_INITIALIZED.store(true, Ordering::SeqCst);
        TRAY_SHUTDOWN.store(false, Ordering::SeqCst);
        TRAY_THREAD_ID.store(42, Ordering::SeqCst);

        reset_tray_state();

        assert!(!TRAY_INITIALIZED.load(Ordering::SeqCst));
        assert!(TRAY_SHUTDOWN.load(Ordering::SeqCst));
        assert_eq!(TRAY_THREAD_ID.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn join_tray_icon_thread_drains_handle() {
        let _guard = test_lock();

        *tray_icon_thread() = Some(thread::spawn(|| {}));

        join_tray_icon_thread();

        assert!(tray_icon_thread().is_none());
    }

    #[test]
    fn join_tray_click_thread_drains_handle() {
        let _guard = test_lock();

        *tray_click_thread() = Some(thread::spawn(|| {}));

        join_tray_click_thread();

        assert!(tray_click_thread().is_none());
    }
}
