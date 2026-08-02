use crate::config::ThemeColor;
use crate::config::{self, CONFIG_PATH};
use crate::core::shared_state::DEVICE_PIDS;
use crate::error::{AppError, AppResult};
use crate::razer::config::{AppConfig, PowerProfile};
use crate::razer::enums::{BatteryLimit, LidLogoMode, PerfMode, RGBEffect};
use crate::razer::executer::Executer;
use crate::razer::protocol::command;
use crate::runtime::settings_state::SettingsState;
use crate::utils::persist::PersistBuffer;
use crate::win::audio::AudioType;
use librazer::descriptor::Descriptor;
use tracing::{error, info, warn};

use librazer::device::Device;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

static MODEL: OnceLock<String> = OnceLock::new();
static DEVICE_CHANNEL_MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);
static DEVICE_CHANNEL_MONITOR_WAKE: OnceLock<Arc<DeviceChannelMonitorWake>> = OnceLock::new();
static DEVICE_CHANNEL_MONITOR_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

type DeviceChannelMonitorWake = (Mutex<()>, Condvar);

// ── Global Singleton ────────────────────────────────────────────────────────

static CMDS_TX: OnceLock<DeviceCommandSenders> = OnceLock::new();

#[derive(Clone)]
struct DeviceCommandSenders {
    normal: mpsc::Sender<DeviceCmd>,
    urgent: mpsc::Sender<DeviceCmd>,
}

/// Returns a `DeviceHandle` connected to the background device thread.
/// On first call, spawns the worker thread that owns the hardware device.
pub fn device() -> DeviceHandle {
    let senders = CMDS_TX.get_or_init(|| {
        let (normal_tx, normal_rx) = mpsc::channel::<DeviceCmd>();
        let (urgent_tx, urgent_rx) = mpsc::channel::<DeviceCmd>();

        if let Err(error) = thread::Builder::new()
            .name("blade-device-worker".to_string())
            .spawn(move || run_device_worker(normal_rx, urgent_rx))
        {
            error!(%error, "Failed to start Razer device worker thread");
        }
        monitor_device_channel();

        DeviceCommandSenders {
            normal: normal_tx,
            urgent: urgent_tx,
        }
    });

    DeviceHandle {
        sender: senders.normal.clone(),
        urgent_sender: senders.urgent.clone(),
    }
}

fn monitor_device_channel() {
    join_finished_device_channel_monitor_thread();

    if DEVICE_CHANNEL_MONITOR_RUNNING.swap(true, Ordering::SeqCst) {
        warn!("Device channel monitor is already running");
        return;
    }

    match thread::Builder::new()
        .name("blade-device-channel-monitor".to_string())
        .spawn(|| {
            info!("Started channel monitor thread");
            wait_for_device_channel_monitor(Duration::from_secs(20));
            while DEVICE_CHANNEL_MONITOR_RUNNING.load(Ordering::SeqCst) {
                let _ = device().get_pid();
                wait_for_device_channel_monitor(Duration::from_mins(2));
            }
        }) {
        Ok(handle) => {
            *device_channel_monitor_thread() = Some(handle);
        }
        Err(error) => {
            DEVICE_CHANNEL_MONITOR_RUNNING.store(false, Ordering::SeqCst);
            error!(%error, "Failed to start device channel monitor thread");
        }
    }
}

pub fn stop_device_channel_monitor() {
    DEVICE_CHANNEL_MONITOR_RUNNING.store(false, Ordering::SeqCst);
    device_channel_monitor_wake().1.notify_all();
    join_device_channel_monitor_thread();
}

fn device_channel_monitor_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    DEVICE_CHANNEL_MONITOR_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_device_channel_monitor_thread() {
    let should_join = device_channel_monitor_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);

    if should_join {
        join_device_channel_monitor_thread();
    }
}

fn join_device_channel_monitor_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = device_channel_monitor_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current device channel monitor thread during shutdown");
        *device_channel_monitor_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("Device channel monitor thread panicked during shutdown");
    }
}

fn device_channel_monitor_wake() -> Arc<DeviceChannelMonitorWake> {
    DEVICE_CHANNEL_MONITOR_WAKE
        .get_or_init(|| Arc::new((Mutex::new(()), Condvar::new())))
        .clone()
}

fn wait_for_device_channel_monitor(duration: Duration) {
    let signal = device_channel_monitor_wake();
    let (lock, cvar) = &*signal;
    let guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (_guard, _timeout) = cvar
        .wait_timeout_while(guard, duration, |_| {
            DEVICE_CHANNEL_MONITOR_RUNNING.load(Ordering::SeqCst)
        })
        .unwrap_or_else(|poisoned| poisoned.into_inner());
}

fn run_device_worker(rx: mpsc::Receiver<DeviceCmd>, urgent_rx: mpsc::Receiver<DeviceCmd>) {
    let device = match get_razer_device() {
        Ok(device) => device,
        Err(error) => {
            error!(%error, "No compatible Razer device found; worker thread exiting");
            return;
        }
    };
    let mut app_config = config::load_config(&device.info);
    let persist_buffer = PersistBuffer::new(CONFIG_PATH.to_string());
    let mut executer = match Executer::new(&device, &mut app_config, persist_buffer, rx, urgent_rx)
    {
        Ok(e) => e,
        Err(e) => {
            error!(error = %e, "Failed to initialise Executer; worker thread exiting");
            return;
        }
    };
    executer.process_commands();
}

fn get_razer_device() -> AppResult<Device> {
    match Device::detect() {
        Ok(d) => Ok(d),
        Err(e) => {
            if let Ok(mut razer_enums) = Device::enumerate() {
                razer_enums.0.sort();
                let (device_pids, device_model) = razer_enums;
                let model = MODEL.get_or_init(|| format!("Razer Blade {}", device_model));
                if DEVICE_PIDS.set(device_pids.clone()).is_err() {
                    warn!("Device PID cache was already initialized");
                }

                // Loop through all detected Razer PIDs and return the first that responds
                for pid in &device_pids {
                    let custom_descriptor = Descriptor {
                        model_number_prefix: model,
                        name: model,
                        pid: *pid,
                        features: &[],
                    };
                    let Ok(device) = Device::new(custom_descriptor) else {
                        continue;
                    };
                    // Command to check performance mode
                    if command(&device, 0x0d82, &[0, 0, 0, 0], Some(&[2, 3])).is_ok() {
                        return Ok(device);
                    }
                }
                Err(AppError::Internal(format!(
                    "No responding Razer HID interface found after fallback detection: {e}"
                )))
            } else {
                Err(AppError::Internal(format!(
                    "Razer device detection and enumeration failed: {e}"
                )))
            }
        }
    }
}

