use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::{NativeOptions, egui};
use rdev::Key;
use tray_icon::TrayIcon;

use crate::core::shared_state::KEYMAP_LISTENING;
use crate::razer::device_handle::{DeviceHandle, device};
use crate::ui::app_events::AppEvent;
use crate::ui::event_dispatcher::{EventDispatcher, SideEffect};
use crate::ui::osd::Osd;
use crate::ui::settings::store::SettingsStore;
use crate::ui::theme::OSD_WINDOW_SIZE;
use crate::ui::tray;
use crate::utils::oncelock_ext::OnceLockExt;
use crate::win::input::key_map::KeyCombo;

// ── Global State ────────────────────────────────────────────────────────────

/// Centralized application context: bundles the event channel sender, the
/// egui rendering context, and the device handle. Replaces the previous
/// scattered globals with a single source of truth.
#[derive(Clone)]
pub struct AppContext {
    pub tx: Sender<AppEvent>,
    pub egui_ctx: egui::Context,
    pub device: DeviceHandle,
}

static APP_CONTEXT: OnceLock<AppContext> = OnceLock::new();

// ── App (eframe application) ────────────────────────────────────────────

struct App {
    // Event channel
    rx: Receiver<AppEvent>,

    // System tray
    tray_icon: TrayIcon,

    // OSD
    osd_init: bool,
    osd: Osd,
    osd_enabled: bool,

    // Settings Window
    settings: SettingsStore,
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(ctx) = APP_CONTEXT.get() {
            let _ = ctx.device.shutdown();
        }
    }
}

impl App {
    fn new(rx: Receiver<AppEvent>) -> Self {
        Self {
            rx,
            tray_icon: tray::build_tray_icon(),
            osd_init: false,
            osd: Osd::new(),
            osd_enabled: true,
            settings: SettingsStore::new(),
        }
    }

    // ── Event Handling ──────────────────────────────────────────────────

    /// Drains pending events and returns whether the OSD should be shown.
    fn handle_app_events(&mut self, ctx: &egui::Context) -> bool {
        let mut wake = false;
        while let Ok(event) = self.rx.try_recv() {
            let (osd_response, side_effect) = EventDispatcher::dispatch(event);

            // Process side effects first
            if let Some(side) = side_effect {
                self.apply_side_effect(side, ctx);
            }

            // Then optionally trigger OSD
            if let Some(response) = osd_response
                && self.osd_enabled
            {
                self.osd.apply_osd_response(response);
                wake = true;
            }
        }
        wake
    }

    /// Applies a side effect from the event dispatcher.
    fn apply_side_effect(&mut self, side: SideEffect, ctx: &egui::Context) {
        match side {
            SideEffect::ToggleSettings => {
                let app_config = APP_CONTEXT
                    .get()
                    .expect("App context not initialized")
                    .device
                    .get_config()
                    .unwrap_or_default();
                self.settings.toggle(app_config);
            }
            SideEffect::OpenSettings => {
                let app_config = APP_CONTEXT
                    .get()
                    .expect("App context not initialized")
                    .device
                    .get_config()
                    .unwrap_or_default();
                self.settings.show(app_config);
            }
            SideEffect::Shutdown => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            SideEffect::EnableOsd(enable) => {
                self.osd_enabled = enable;
            }
            SideEffect::RazerKeyCode(key_code) => {
                if KEYMAP_LISTENING.load(Ordering::SeqCst) {
                    self.settings.set_razer_key_code(key_code);
                }
            }
            SideEffect::PerfMode(mode) => {
                tray::set_perf_mode_icon(&mut self.tray_icon, mode);
            }
        }
    }
}

// ── eframe::App Implementation ──────────────────────────────────────────────

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        if is_focused {
            unfocus();
        }
        self.settings.run(ctx);

        let trigger_osd = self.handle_app_events(ctx);
        self.osd.run(ctx, trigger_osd);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

// ── Public Handle ───────────────────────────────────────────────────────────

/// A handle for sending `AppEvent`s to the application, requesting
/// repaints on the egui context, and accessing the device layer.
/// All internal fields are reference-counted, so cloning is cheap.
#[derive(Clone)]
pub struct AppHandle {
    tx: Sender<AppEvent>,
    pub egui_ctx: egui::Context,
    pub device: DeviceHandle,
}

impl AppHandle {
    pub fn send(&self, event: AppEvent) {
        self.tx
            .send(event)
            .expect("Fatal internal error: App TX send");
        self.egui_ctx.request_repaint();
    }
}

/// Returns a handle to the running `App` event channel.
pub fn app() -> AppHandle {
    let ctx = APP_CONTEXT
        .get_or_timeout()
        .expect("Fatal internal error: App context initialization timeout");
    AppHandle {
        tx: ctx.tx.clone(),
        egui_ctx: ctx.egui_ctx.clone(),
        device: ctx.device.clone(),
    }
}

// ── Entry Point ─────────────────────────────────────────────────────────────

/// Launches the eframe-based OSD overlay and tray icon application.
pub fn run() {
    let (tx, rx) = mpsc::channel::<AppEvent>();
    eframe::run_native(
        "Blade ControlHub OSD",
        native_options(),
        Box::new(move |cc| {
            let device_handle = device();
            let _ = APP_CONTEXT.set(AppContext {
                tx,
                egui_ctx: cc.egui_ctx.clone(),
                device: device_handle,
            });
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Box::new(App::new(rx))
        }),
    )
    .expect("Fatal internal error: App error");
}

// ── Window Options ──────────────────────────────────────────────────────────

/// Returns the `NativeOptions` for the OSD overlay window.
fn native_options() -> NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_active(false)
            .with_inner_size([OSD_WINDOW_SIZE.x, OSD_WINDOW_SIZE.y])
            .with_taskbar(false),

        // Use standard sync/acceleration
        vsync: true,
        hardware_acceleration: eframe::HardwareAcceleration::Required,
        ..Default::default()
    }
}

fn unfocus() {
    KeyCombo::new(&[Key::Alt, Key::Tab, Key::Escape]).trigger();
}
