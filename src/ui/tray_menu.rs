/// Tray menu construction, event handling, and click listener.
///
/// Handles context menu item definitions, menu click event routing,
/// and the background thread that listens for tray icon clicks.
use crate::ui::app::app;
use crate::ui::app_events::{AppEvent, OsdEvent};
use crate::utils::reload::restart_app;
use crate::win::system::startup::Startup;
use tracing::debug;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

// ── Startup State ───────────────────────────────────────────────────────────

static STARTUP_STATE: AtomicBool = AtomicBool::new(false);

// ── Menu Construction ───────────────────────────────────────────────────────

/// Builds the tray context menu with Quit, Restart, Settings, Startup toggle,
/// and Default Multimedia Keys items.
pub fn build_tray_menu() -> Menu {
    let tray_menu = Menu::new();

    let quit_item = MenuItem::with_id("quit", "Quit", true, None);
    let restart_item = MenuItem::with_id("restart", "Restart", true, None);
    let settings_item = MenuItem::with_id("settings_window", "Settings", true, None);

    let startup_detected = Startup::is_registered();
    let default_multimedia_keys = app()
        .device
        .get_default_multimedia_keys()
        .unwrap_or_default();
    let startup_item = CheckMenuItem::with_id(
        "startup_toggle",
        "Start with Windows",
        true,
        startup_detected,
        None,
    );
    let default_keys_item = CheckMenuItem::with_id(
        "default_multimedia_keys",
        "Default Multimedia Keys",
        true,
        default_multimedia_keys,
        None,
    );
    STARTUP_STATE.store(startup_detected, Ordering::SeqCst);

    tray_menu.append(&quit_item).unwrap();
    tray_menu.append(&restart_item).unwrap();
    tray_menu.append(&settings_item).unwrap();
    tray_menu.append(&startup_item).unwrap();
    tray_menu.append(&default_keys_item).unwrap();

    tray_menu
}

/// Registers the global menu event handler for tray menu actions.
pub fn setup_menu_event_handler() {
    let startup_state = Arc::new(&STARTUP_STATE);

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id.0.as_str() {
        "quit" => {
            app().send(AppEvent::Shutdown);
        }
        "restart" => {
            restart_app(0);
        }
        "settings_window" => app().send(AppEvent::OpenSettings),
        "startup_toggle" => {
            let is_checked = !startup_state.load(Ordering::SeqCst);
            startup_state.store(is_checked, Ordering::SeqCst);
            if is_checked && !Startup::is_registered() {
                Startup::register();
            } else if !is_checked && Startup::is_registered() {
                Startup::unregister();
            }
        }
        "default_multimedia_keys" => {
            let new_default = app()
                .device
                .toggle_default_multimedia_keys()
                .unwrap_or_default();
            app().send(OsdEvent::ToggleDefaultMultimediaKeys(new_default).into())
        }
        _ => {}
    }));
}

// ── Tray Click Listener ─────────────────────────────────────────────────────

/// Spawns a background thread that listens for left-clicks on the tray icon
/// and triggers a settings toggle event.
pub fn spawn_tray_click_listener() {
    thread::spawn(move || {
        loop {
            if let Ok(TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }) = TrayIconEvent::receiver().recv()
            {
                debug!("Tray icon clicked, toggling settings");
                app().send(AppEvent::ToggleSettings);
            }
        }
    });
}
