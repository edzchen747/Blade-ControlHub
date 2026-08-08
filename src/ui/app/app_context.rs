use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use crate::razer::device_handle::{DeviceHandle, device, stop_device_channel_monitor};
use crate::runtime::launch_args::SETTINGS_MODE_ARG;
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

#[derive(Clone)]
pub struct AppContext {
    pub core: Arc<Mutex<AppCore>>,
    pub device: DeviceHandle,
}

static APP_CONTEXT: OnceLock<AppContext> = OnceLock::new();

static ACTIVE_UI_PROCESS: Mutex<Option<Child>> = Mutex::new(None);
static SHUTDOWN_SIGNAL: OnceLock<Arc<ShutdownSignal>> = OnceLock::new();

type ShutdownSignal = (Mutex<bool>, Condvar);

pub struct AppCore {
    pub running: bool,
    pub osd_enabled: bool,
    osd_disable_guards: usize,
    settings_osd_disable_guard: Option<OsdDisableGuard>,
    pub settings: SettingsStore,
    pub pending_side_effects: Vec<SideEffect>,
}

