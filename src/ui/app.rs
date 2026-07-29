use std::process::{Child, Command};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};

use crate::core::shared_state::KEYMAP_LISTENING;
use crate::razer::device_handle::{DeviceHandle, device};
use crate::ui::app_events::AppEvent::{self, OsdEvent};
use crate::ui::event_dispatcher::{EventDispatcher, SideEffect};
use crate::ui::osd_controller::OsdController;
use crate::ui::settings::store::SettingsStore;
use crate::ui::tray::TrayManager;
use crate::utils::oncelock_ext::OnceLockExt;

// ── Global Context ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppContext {
    pub core: Arc<Mutex<AppCore>>,
    pub device: DeviceHandle,
}

static APP_CONTEXT: OnceLock<AppContext> = OnceLock::new();

static mut ACTIVE_UI_PROCESS: Option<Child> = None;

// ── App Core State Structure ──────────────────────────────────────────────────

pub struct AppCore {
    pub running: bool,
    pub osd_enabled: bool,
    pub settings: SettingsStore,
    pub pending_side_effects: Vec<SideEffect>,
}

impl AppCore {
    fn new() -> Self {
        Self {
            running: true,
            osd_enabled: true,
            settings: SettingsStore::new(),
            pending_side_effects: Vec::new(),
        }
    }
}

// ── Public Dispatcher Entrypoint ─────────────────────────────────────────────

pub fn app(event: AppEvent) {
    let ctx = APP_CONTEXT
        .get_or_timeout()
        .expect("Fatal internal error: App context initialization timeout");

    // 1. Dispatch incoming events and capture matching platform side effects
    if let Some(side) = EventDispatcher::dispatch(event) {
        if let Ok(mut core) = ctx.core.lock() {
            core.pending_side_effects.push(side);
        }
    }

    // 2. Process all pending side effects immediately within the event transaction block
    let pending = {
        let mut core = ctx.core.lock().unwrap();
        if !core.running {
            std::process::exit(0);
        }
        std::mem::take(&mut core.pending_side_effects)
    };

    for side in pending {
        match side {
            SideEffect::ToggleSettings | SideEffect::OpenSettings => {
                let app_config = ctx.device.get_config().unwrap_or_default();

                {
                    let mut core = ctx.core.lock().unwrap();
                    core.settings.show(app_config);
                }

                unsafe {
                    if let Some(ref mut child) = ACTIVE_UI_PROCESS {
                        if child.try_wait().unwrap().is_none() {
                            let _ = child.kill();
                            ACTIVE_UI_PROCESS = None;
                            return;
                        }
                    }

                    let child = Command::new(
                        std::env::current_exe()
                            .unwrap()
                            .with_file_name("blade_settings"),
                    )
                    .spawn()
                    .expect("Failed to execute independent blade_settings window sub-process");

                    ACTIVE_UI_PROCESS = Some(child);
                }
            }
            SideEffect::Shutdown => {
                ctx.core.lock().unwrap().running = false;
                std::process::exit(0);
            }
            SideEffect::EnableOsd(enable) => {
                ctx.core.lock().unwrap().osd_enabled = enable;
            }
            SideEffect::RazerKeyCode(key_code) => {
                if KEYMAP_LISTENING.load(Ordering::SeqCst) {
                    let mut core = ctx.core.lock().unwrap();
                    core.settings.set_razer_key_code(key_code);
                }
            }
            SideEffect::PerfMode(mode) => {
                TrayManager::set_tray_icon(mode);
            }
        }
    }

    // 3. Process standalone On-Screen Display Overlay parameters via static invocation
    if let OsdEvent(osd_event) = event {
        if let Some(osd_params) = osd_event.as_params() {
            let core = ctx.core.lock().unwrap();
            if core.osd_enabled {
                OsdController::show(osd_params);
            }
        }
    }
}

pub fn run() {
    let device_handle = device();
    let shared_core = Arc::new(Mutex::new(AppCore::new()));

    let _ = APP_CONTEXT.set(AppContext {
        core: shared_core,
        device: device_handle,
    });

    std::thread::park();
}
