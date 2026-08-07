use crate::config::ThemeColor;
use crate::ipc::client;
use crate::razer::enums::PerfMode;
use crate::runtime::settings_state::SettingsState;
use crate::runtime::settings_updates::{self, SettingsUpdateListener};
use crate::ui::settings::store::SettingsStore;
use crate::ui::settings::{Settings, SettingsCommand};
use crate::ui::theme::{
    SETTINGS_KEY_LISTEN_INTERVAL_MS, SETTINGS_LOADING_ICON_COLOR, SETTINGS_PADDING_RATIO,
    SETTINGS_WINDOW_SIZE, SETTINGS_WINDOW_TITLE,
};
use crate::utils::log_file::{init_log_file_writer_for_child, set_cwd};
use eframe::egui;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

const SETTINGS_FRAME_INTERVAL: Duration = Duration::from_millis(16);
const SETTINGS_UPDATE_WAIT_INTERVAL: Duration = Duration::from_millis(100);
const SETTINGS_STATE_INITIAL_RETRY_INTERVAL: Duration = Duration::from_millis(500);
const SETTINGS_UPDATE_DEBOUNCE: Duration = Duration::from_millis(50);
const WINDOWS_DEFAULT_DPI: u32 = 96;

struct SettingsApp {
    settings: SettingsStore,
    state_loaded: bool,
    last_frame_at: Option<Instant>,
    settings_update_tx: Sender<SettingsUpdateMessage>,
    settings_update_rx: Receiver<SettingsUpdateMessage>,
    settings_update_shutdown: Arc<AtomicBool>,
    settings_update_thread: Option<thread::JoinHandle<()>>,
    razer_key_capture_tx: Sender<RazerKeyCaptureMessage>,
    razer_key_capture_rx: Receiver<RazerKeyCaptureMessage>,
    razer_key_capture_cancel: Option<Arc<AtomicBool>>,
    razer_key_capture_id: u64,
    active_razer_key_capture_id: Option<u64>,
    command_lab_record_tx: Sender<CommandLabRecordMessage>,
    command_lab_record_rx: Receiver<CommandLabRecordMessage>,
    command_lab_record_cancel: Option<Arc<AtomicBool>>,
    command_lab_record_id: u64,
    active_command_lab_record_id: Option<u64>,
    applied_window_icon_color: Option<ThemeColor>,
    native_window_icons: Option<NativeWindowIcons>,
    reported_window_focus: Option<bool>,
}


include!("settings_app_impl.rs");
include!("settings_app_drop.rs");
