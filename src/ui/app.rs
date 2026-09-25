//! The runtime's event hub.
//!
//! Every surface that can change what the user sees — hotkeys, power and
//! display monitors, the tray, the settings window — funnels through [`app`].
//! It resolves the event into at most one side effect, then shows the OSD if
//! the event has an overlay and the OSD is not suppressed.
//!
//! Suppression is a property of the command's origin, not of what has focus:
//! the settings window's own controls suppress the overlay they would raise
//! (see `Executer::dispatch`), while the Razer keys, Fn combinations and the
//! Windows monitors keep theirs even while that window is open and focused.
//!
//! Since the Tauri rebuild this all lives in one process: the settings window
//! is a webview owned by the same runtime rather than a child process, so
//! opening it is a window show rather than a spawn.

#[macro_export]
macro_rules! disable_osd {
    ($($body:tt)*) => {{
        let _osd_disable_guard = $crate::ui::app::OsdDisableGuard::new();
        { $($body)* }
    }};
}

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use tracing::{debug, info, warn};

use crate::razer::device_handle::{DeviceHandle, device, stop_device_channel_monitor};
use crate::ui::app_events::{AppEvent, OsdEvent};
use crate::ui::event_dispatcher::{EventDispatcher, SideEffect};
use crate::ui::osd_controller::OsdController;
use crate::ui::webui;
use crate::utils::oncelock_ext::OnceLockExt;
use crate::utils::reload::spawn_replacement_app;
use crate::win::external_events::ExternalChangeMonitor;
use crate::win::input::stop_keyboard_hooks;
use crate::win::system::display_gpu::GpuDisplayMonitor;
use crate::win::system::power::PowerMonitor;
use crate::win::system::standby::StandbyMonitor;

#[derive(Clone)]
pub struct AppContext {
    pub core: Arc<Mutex<AppCore>>,
    pub device: DeviceHandle,
}

pub struct AppCore {
    pub running: bool,
    pub osd_enabled: bool,
    osd_disable_guards: usize,
    pub pending_side_effects: Vec<SideEffect>,
}

static APP_CONTEXT: OnceLock<AppContext> = OnceLock::new();

impl AppCore {
    fn new() -> Self {
        Self {
            running: true,
            osd_enabled: true,
            osd_disable_guards: 0,
            pending_side_effects: Vec::new(),
        }
    }

    fn should_show_osd(&self) -> bool {
        self.osd_enabled && self.osd_disable_guards == 0
    }
}

pub fn init() {
    let _ = APP_CONTEXT.get_or_init(|| AppContext {
        core: Arc::new(Mutex::new(AppCore::new())),
        device: device(),
    });
}

/// Hands the main thread to the Tauri event loop, which owns the tray and the
/// settings window. Returns once the app has been asked to exit.
pub fn run() {
    init();
    webui::run();
}

pub fn app(event: AppEvent) {
    let Some(ctx) = APP_CONTEXT.get_or_timeout() else {
        warn!("Dropping app event before app context initialization completed");
        return;
    };

    if let Some(side) = EventDispatcher::dispatch(&event) {
        core(&ctx).pending_side_effects.push(side);
    }

    let pending = {
        let mut core = core(&ctx);
        if !core.running {
            return;
        }
        std::mem::take(&mut core.pending_side_effects)
    };

    for side in pending {
        match side {
            SideEffect::ToggleSettings => webui::toggle_window(),
            SideEffect::OpenSettings => webui::show_window(),
            SideEffect::Shutdown => {
                shutdown_runtime(&ctx, "application shutdown");
                return;
            }
            SideEffect::Restart(code) => {
                if let Err(error) = spawn_replacement_app(code) {
                    warn!(%error, "Failed to spawn replacement app for restart");
                    return;
                }
                shutdown_runtime(&ctx, "application restart");
                return;
            }
            SideEffect::EnableOsd(enable) => {
                core(&ctx).osd_enabled = enable;
            }
            SideEffect::PerfMode(mode) => {
                webui::set_perf_mode_icon(mode);
            }
        }
    }

    let osd_params = match &event {
        AppEvent::OsdEvent(osd_event) => osd_event.as_params(),
        _ => None,
    };
    if let Some(osd_params) = osd_params {
        // Say why an overlay was dropped. Suppression is invisible by nature —
        // a missing overlay looks exactly like a key that did nothing — so the
        // guard count is worth stating rather than inferring.
        let (show, guards, enabled) = {
            let core = core(&ctx);
            (
                core.should_show_osd(),
                core.osd_disable_guards,
                core.osd_enabled,
            )
        };
        if show {
            OsdController::show(osd_params);
        } else {
            debug!(guards, enabled, "Suppressed an OSD overlay");
        }
    }

    // An event that reached here may have moved device state — a hotkey, a
    // power transition, an external display change. Let the window reconcile.
    if matches!(event, AppEvent::OsdEvent(_)) {
        webui::notify_settings_changed();
    }
}

pub fn set_osd_enabled(enabled: bool) {
    app(OsdEvent::EnableOSD(enabled).into());
}

fn core(ctx: &AppContext) -> MutexGuard<'_, AppCore> {
    ctx.core
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn shutdown_runtime(ctx: &AppContext, reason: &str) {
    info!(reason, "Shutting down the Blade ControlHub runtime");
    webui::stop_command_lab_recording();
    webui::stop_razer_key_capture();
    ExternalChangeMonitor::stop();
    OsdController::stop();
    PowerMonitor::stop();
    StandbyMonitor::stop();
    GpuDisplayMonitor::stop();
    stop_keyboard_hooks();
    stop_device_channel_monitor();
    if let Err(error) = ctx.device.shutdown() {
        warn!(%error, reason, "Device shutdown cleanup did not complete");
    }
    core(ctx).running = false;
    webui::request_exit();
}

// ── OSD suppression guards ───────────────────────────────────────────────────

fn acquire_osd_disable_guard() {
    let Some(ctx) = APP_CONTEXT.get() else {
        warn!("Ignoring OSD disable before app initialization");
        return;
    };
    let mut core = core(ctx);
    core.osd_disable_guards = core.osd_disable_guards.saturating_add(1);
}

fn release_osd_disable_guard() {
    let Some(ctx) = APP_CONTEXT.get() else {
        return;
    };
    let mut core = core(ctx);
    if core.osd_disable_guards == 0 {
        warn!("Attempted to release an OSD disable guard that was not held");
        return;
    }
    core.osd_disable_guards -= 1;
}

pub struct OsdDisableGuard;

impl OsdDisableGuard {
    pub fn new() -> Self {
        acquire_osd_disable_guard();
        Self
    }
}

impl Default for OsdDisableGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OsdDisableGuard {
    fn drop(&mut self) {
        release_osd_disable_guard();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osd_disable_guard_count_suppresses_osd_without_changing_user_preference() {
        let mut core = AppCore::new();
        assert!(core.should_show_osd());

        core.osd_disable_guards = 1;
        assert!(!core.should_show_osd());

        core.osd_disable_guards = 0;
        assert!(core.should_show_osd());
    }
}
