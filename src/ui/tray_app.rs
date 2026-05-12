use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::{NativeOptions, egui};
use tray_icon::TrayIcon;

use crate::razer::device_handle::device;
use crate::ui::app_events::*;
use crate::ui::icon;
use crate::ui::osd::*;
use crate::ui::settings::Settings;
use crate::utils::oncelock_ext::OnceLockExt;

// ── Global State ────────────────────────────────────────────────────────────

static TRAY_APP_TX: OnceLock<Sender<OsdEvent>> = OnceLock::new();
static OSD_CONTEXT: OnceLock<egui::Context> = OnceLock::new();

// ── TrayApp (eframe application) ────────────────────────────────────────────

struct TrayApp {
    // Event channel
    rx: Receiver<OsdEvent>,

    // System tray
    tray_icon: TrayIcon,

    // OSD
    osd: OSD,

    // Configurator Window
    config_window: Arc<Mutex<Settings>>,
}

impl TrayApp {
    fn new(rx: Receiver<OsdEvent>) -> Self {
        Self {
            rx,
            tray_icon: icon::build_tray_icon(),
            osd: OSD::new(),
            config_window: Arc::new(Mutex::new(Settings::new())),
        }
    }

    // ── Event Handling ──────────────────────────────────────────────────

    /// Drains pending events and returns whether the OSD should be shown.
    fn poll_osd_events(&mut self) -> bool {
        let mut wake = false;
        while let Ok(event) = self.rx.try_recv() {
            if event == OsdEvent::OpenSettings {
                let app_config = device().get_config();
                self.config_window.lock().unwrap().show(app_config);
            }
            if let Some(response) = process_event(event, &mut self.tray_icon) {
                self.osd.apply_osd_response(response);
                wake = true;
            }
        }
        wake
    }
}

// ── eframe::App Implementation ──────────────────────────────────────────────

impl eframe::App for TrayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        Settings::run(ctx, self.config_window.clone());

        let trigger_osd = self.poll_osd_events();
        self.osd.run(ctx, trigger_osd);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

// ── Public Handle ───────────────────────────────────────────────────────────

/// A clonable handle for sending `OsdEvent`s to the OSD application.
pub struct TrayAppHandle {
    tx: Sender<OsdEvent>,
}

impl TrayAppHandle {
    pub fn send(&self, event: OsdEvent) {
        self.tx
            .send(event)
            .expect("Fatal internal error: OSD TX send");
        OSD_CONTEXT
            .get()
            .expect("Fatal internal error: OSD Context get error")
            .request_repaint();
    }
}

/// Returns a handle to the running `TrayApp` event channel.
pub fn tray_app() -> TrayAppHandle {
    let tx = TRAY_APP_TX
        .get_or_timeout()
        .expect("Fatal internal error: TrayApp channel initialization timeout");
    TrayAppHandle { tx: tx.clone() }
}

// ── Entry Point ─────────────────────────────────────────────────────────────

/// Launches the eframe-based OSD overlay and tray icon application.
pub fn run() {
    let (tx, rx) = mpsc::channel::<OsdEvent>();
    eframe::run_native(
        "Blade ControlHub OSD",
        native_options(),
        Box::new(move |cc| {
            TRAY_APP_TX
                .set(tx)
                .expect("Fatal internal error: OSD channel initialize error");
            OSD_CONTEXT
                .set(cc.egui_ctx.clone())
                .expect("Fatal internal error: OSD Context initialize error");
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Box::new(TrayApp::new(rx))
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
