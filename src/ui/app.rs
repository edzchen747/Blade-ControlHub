use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use crate::razer::device_handle::{DeviceHandle, device, stop_device_channel_monitor};
use crate::ui::app_events::{AppEvent, OsdEvent};
use crate::ui::event_dispatcher::{EventDispatcher, SideEffect};
use crate::ui::osd_controller::OsdController;
use crate::ui::settings::store::SettingsStore;
use crate::ui::tray::TrayManager;
use crate::utils::oncelock_ext::OnceLockExt;
use crate::utils::reload::spawn_replacement_app;
use crate::win::external_events::ExternalChangeMonitor;
use crate::win::input::stop_keyboard_hooks;
use crate::win::system::display_gpu::GpuDisplayMonitor;
use crate::win::system::power::PowerMonitor;
use crate::win::system::standby::StandbyMonitor;
use tracing::{debug, info, warn};

// ── Global Context ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppContext {
    pub core: Arc<Mutex<AppCore>>,
    pub device: DeviceHandle,
}

static APP_CONTEXT: OnceLock<AppContext> = OnceLock::new();

static ACTIVE_UI_PROCESS: Mutex<Option<Child>> = Mutex::new(None);
static SHUTDOWN_SIGNAL: OnceLock<Arc<ShutdownSignal>> = OnceLock::new();

type ShutdownSignal = (Mutex<bool>, Condvar);

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
    let Some(ctx) = APP_CONTEXT.get_or_timeout() else {
        warn!("Dropping app event before app context initialization completed");
        return;
    };

    // 1. Dispatch incoming events and capture matching platform side effects
    if let Some(side) = EventDispatcher::dispatch(event) {
        core(&ctx).pending_side_effects.push(side);
    }

    // 2. Process all pending side effects immediately within the event transaction block
    let pending = {
        let mut core = core(&ctx);
        if !core.running {
            return;
        }
        std::mem::take(&mut core.pending_side_effects)
    };

    for side in pending {
        match side {
            SideEffect::ToggleSettings | SideEffect::OpenSettings => {
                open_or_toggle_settings(&ctx);
            }
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
                TrayManager::set_tray_icon(mode);
            }
        }
    }

    // 3. Process standalone On-Screen Display Overlay parameters via static invocation
    let osd_params = match event {
        AppEvent::OsdEvent(osd_event) => osd_event.as_params(),
        _ => None,
    };
    if let Some(osd_params) = osd_params {
        let core = core(&ctx);
        if core.osd_enabled {
            OsdController::show(osd_params);
        }
    }
}

pub fn set_osd_enabled(enabled: bool) {
    app(OsdEvent::EnableOSD(enabled).into());
}

pub struct OsdDisableGuard;

impl OsdDisableGuard {
    pub fn new() -> Self {
        set_osd_enabled(false);
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
        set_osd_enabled(true);
    }
}

#[macro_export]
macro_rules! disable_osd {
    ($($body:tt)*) => {{
        let _osd_disable_guard = $crate::ui::app::OsdDisableGuard::new();
        { $($body)* }
    }};
}

pub fn init() {
    let _ = APP_CONTEXT.get_or_init(|| AppContext {
        core: Arc::new(Mutex::new(AppCore::new())),
        device: device(),
    });
    crate::ipc::server::start(device());
    let _ = shutdown_signal();
}

pub fn run() {
    init();
    let shutdown_signal = shutdown_signal();
    wait_for_shutdown(&shutdown_signal);
}

fn active_ui_process() -> MutexGuard<'static, Option<Child>> {
    ACTIVE_UI_PROCESS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn core(ctx: &AppContext) -> MutexGuard<'_, AppCore> {
    ctx.core
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn shutdown_signal() -> Arc<ShutdownSignal> {
    SHUTDOWN_SIGNAL
        .get_or_init(|| Arc::new((Mutex::new(false), Condvar::new())))
        .clone()
}

fn signal_shutdown() {
    let signal = shutdown_signal();
    let (lock, cvar) = &*signal;
    let mut should_shutdown = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *should_shutdown = true;
    cvar.notify_all();
}

fn wait_for_shutdown(signal: &ShutdownSignal) {
    let (lock, cvar) = signal;
    let mut should_shutdown = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    while !*should_shutdown {
        should_shutdown = cvar
            .wait(should_shutdown)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    }
}

fn shutdown_runtime(ctx: &AppContext, reason: &str) {
    TrayManager::shutdown();
    ExternalChangeMonitor::stop();
    OsdController::stop();
    PowerMonitor::stop();
    StandbyMonitor::stop();
    GpuDisplayMonitor::stop();
    stop_keyboard_hooks();
    crate::ipc::server::stop();
    stop_device_channel_monitor();
    if let Err(error) = ctx.device.shutdown() {
        warn!(%error, reason, "Device shutdown cleanup did not complete");
    }
    terminate_active_settings_process(reason);
    core(ctx).running = false;
    signal_shutdown();
}

fn open_or_toggle_settings(ctx: &AppContext) {
    let total_started = Instant::now();
    let state_started = Instant::now();
    debug!("Loading settings state before launching settings window");
    let settings_state = match ctx.device.get_settings_state() {
        Ok(state) => {
            info!(
                elapsed_ms = state_started.elapsed().as_millis() as u64,
                "Loaded settings state before launching settings window"
            );
            state
        }
        Err(error) => {
            warn!(
                %error,
                elapsed_ms = state_started.elapsed().as_millis() as u64,
                "Failed to load settings state before launching settings window; using defaults"
            );
            Default::default()
        }
    };

    {
        let core = core(ctx);
        core.settings.show(settings_state);
    }

    let mut active_ui_process = active_ui_process();
    if close_running_settings_process_if_present(&mut active_ui_process) {
        info!(
            elapsed_ms = total_started.elapsed().as_millis() as u64,
            "Closed running settings window process"
        );
        return;
    }

    let settings_exe = match settings_executable_path() {
        Ok(path) => path,
        Err(error) => {
            warn!(%error, "Failed to resolve settings executable path");
            return;
        }
    };

    let spawn_started = Instant::now();
    info!(
        path = ?settings_exe,
        elapsed_ms = total_started.elapsed().as_millis() as u64,
        "Launching settings window process"
    );
    match Command::new(&settings_exe).spawn() {
        Ok(child) => {
            info!(
                pid = child.id(),
                spawn_elapsed_ms = spawn_started.elapsed().as_millis() as u64,
                total_elapsed_ms = total_started.elapsed().as_millis() as u64,
                "Settings window process spawned"
            );
            *active_ui_process = Some(child);
        }
        Err(error) => warn!(
            %error,
            path = ?settings_exe,
            spawn_elapsed_ms = spawn_started.elapsed().as_millis() as u64,
            total_elapsed_ms = total_started.elapsed().as_millis() as u64,
            "Failed to launch settings window"
        ),
    }
}

fn close_running_settings_process_if_present(active_ui_process: &mut Option<Child>) -> bool {
    let Some(child) = active_ui_process.as_mut() else {
        return false;
    };

    match child_state(child) {
        ChildState::Running => {
            terminate_child(child, "settings toggle");
            *active_ui_process = None;
            true
        }
        ChildState::Exited(status) => {
            warn!(?status, "Settings process had already exited");
            *active_ui_process = None;
            false
        }
        ChildState::Unknown => {
            *active_ui_process = None;
            false
        }
    }
}

fn terminate_active_settings_process(reason: &str) {
    let mut active_ui_process = active_ui_process();
    let Some(mut child) = active_ui_process.take() else {
        return;
    };

    if matches!(child_state(&mut child), ChildState::Running) {
        terminate_child(&mut child, reason);
    }
}

fn terminate_child(child: &mut Child, reason: &str) {
    if let Err(error) = child.kill() {
        warn!(%error, reason, "Failed to terminate settings process");
    }
}

enum ChildState {
    Running,
    Exited(ExitStatus),
    Unknown,
}

fn child_state(child: &mut Child) -> ChildState {
    match child.try_wait() {
        Ok(None) => ChildState::Running,
        Ok(Some(status)) => ChildState::Exited(status),
        Err(error) => {
            warn!(%error, "Failed to query settings process state");
            ChildState::Unknown
        }
    }
}

fn settings_executable_path() -> std::io::Result<PathBuf> {
    Ok(settings_executable_path_from(std::env::current_exe()?))
}

fn settings_executable_path_from(current_exe: PathBuf) -> PathBuf {
    let file_name = format!("blade-settings{}", std::env::consts::EXE_SUFFIX);
    current_exe.with_file_name(file_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn settings_executable_path_is_next_to_current_binary() {
        let path = settings_executable_path_from(PathBuf::from(
            r"C:\Tools\Blade\target\debug\blade-controlhub.exe",
        ));

        assert_eq!(
            path,
            Path::new(r"C:\Tools\Blade\target\debug")
                .join(format!("blade-settings{}", std::env::consts::EXE_SUFFIX))
        );
    }
}
