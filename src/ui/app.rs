use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::{NativeOptions, egui};
use tray_icon::TrayIcon;

use crate::razer::device_handle::device;
use crate::ui::app_events::*;
use crate::ui::osd::*;
use crate::ui::settings::Settings;
use crate::ui::tray;
use crate::utils::oncelock_ext::OnceLockExt;

// ── Global State ────────────────────────────────────────────────────────────

static APP_TX: OnceLock<Sender<AppEvent>> = OnceLock::new();
static OSD_CONTEXT: OnceLock<egui::Context> = OnceLock::new();

// ── App (eframe application) ────────────────────────────────────────────

struct App {
    // Event channel
    rx: Receiver<AppEvent>,

    // System tray
    tray_icon: TrayIcon,

    // OSD
    osd: Osd,
    osd_enabled: bool,

    // Settings Window
    settings: Arc<Mutex<Settings>>,
}

impl Drop for App {
    fn drop(&mut self) {
        device().shutdown();
    }
}

impl App {
    fn new(rx: Receiver<AppEvent>) -> Self {
        Self {
            rx,
            tray_icon: tray::build_tray_icon(),
            osd: Osd::new(),
            osd_enabled: true,
            settings: Arc::new(Mutex::new(Settings::new())),
        }
    }

    // ── Event Handling ──────────────────────────────────────────────────

    /// Drains pending events and returns whether the OSD should be shown.
    fn handle_app_events(&mut self, ctx: &egui::Context) -> bool {
        let mut wake = false;
        while let Ok(event) = self.rx.try_recv() {
            match event {
                AppEvent::ToggleSettings => {
                    let app_config = device().get_config();
                    self.settings.lock().unwrap().toggle(app_config);
                }
                AppEvent::OpenSettings => {
                    let app_config = device().get_config();
                    self.settings.lock().unwrap().show(app_config);
                }
                AppEvent::Shutdown => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                AppEvent::OsdEvent(OsdEvent::EnableOSD(enable)) => {
                    self.osd_enabled = enable;
                }
                AppEvent::RazerKeyCode(key_code) => {
                    let is_listening = self
                        .settings
                        .lock()
                        .unwrap()
                        .custom_key_map
                        .listening_idx
                        .is_some();
                    if is_listening {
                        self.settings.lock().unwrap().custom_key_map.special_key = Some(key_code);
                    }
                }
                AppEvent::OsdEvent(_) => (),
            }
            if let Some(response) = process_osd_event(event, &mut self.tray_icon)
                && self.osd_enabled
            {
                self.osd.apply_osd_response(response);
                wake = true;
            }
        }
        wake
    }
}

// ── eframe::App Implementation ──────────────────────────────────────────────

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        Settings::run(ctx, self.settings.clone());

        let trigger_osd = self.handle_app_events(ctx);
        self.osd.run(ctx, trigger_osd);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

// ── Public Handle ───────────────────────────────────────────────────────────

/// A clonable handle for sending `OsdEvent`s to the OSD application.
pub struct AppHandle {
    tx: Sender<AppEvent>,
}

impl AppHandle {
    pub fn send(&self, event: AppEvent) {
        self.tx
            .send(event)
            .expect("Fatal internal error: OSD TX send");
        OSD_CONTEXT
            .get()
            .expect("Fatal internal error: OSD Context get error")
            .request_repaint();
    }
}

/// Returns a handle to the running `App` event channel.
pub fn app() -> AppHandle {
    let tx = APP_TX
        .get_or_timeout()
        .expect("Fatal internal error: App channel initialization timeout");
    AppHandle { tx: tx.clone() }
}

// ── Entry Point ─────────────────────────────────────────────────────────────

/// Launches the eframe-based OSD overlay and tray icon application.
pub fn run() {
    let (tx, rx) = mpsc::channel::<AppEvent>();
    eframe::run_native(
        "Blade ControlHub OSD",
        native_options(),
        Box::new(move |cc| {
            APP_TX
                .set(tx)
                .expect("Fatal internal error: OSD channel initialize error");
            OSD_CONTEXT
                .set(cc.egui_ctx.clone())
                .expect("Fatal internal error: OSD Context initialize error");
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Box::new(App::new(rx))
        }),
    )
    .expect("Fatal internal error: OSD error");
}

// ── Window Options ──────────────────────────────────────────────────────────

/// Returns the `NativeOptions` for the OSD overlay window.
fn native_options() -> NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_inner_size([OSD_WINDOW_SIZE.x, OSD_WINDOW_SIZE.y])
            .with_taskbar(false),

        // Use standard sync/acceleration
        vsync: true,
        hardware_acceleration: eframe::HardwareAcceleration::Required,
        ..Default::default()
    }
}
