use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use resvg::{tiny_skia, usvg};
use tracing::{debug, warn};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, PostThreadMessageW,
    QS_ALLINPUT, TranslateMessage, WM_NULL, WM_QUIT,
};

use crate::razer::enums::PerfMode;
use crate::ui::app::app;
use crate::ui::app_events::AppEvent;
use crate::ui::theme::{
    APP_TOOLTIP, DEFAULT_ICON_COLOR, TRAY_ICON_SCALE_FACTOR, TRAY_ICON_SIZE, perf_mode_hex_color,
};
use crate::win::system::cli_utils::cycle_gpu;

// ── Globals & State ──────────────────────────────────────────────────────────

static TRAY_INITIALIZED: AtomicBool = AtomicBool::new(false);
static TRAY_SHUTDOWN: AtomicBool = AtomicBool::new(false);
static TRAY_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static TRAY_UPDATE_SENDER: Mutex<Option<Sender<PerfMode>>> = Mutex::new(None);
static TRAY_ICON_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static TRAY_CLICK_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

// ── Tray Manager Struct ──────────────────────────────────────────────────────

pub struct TrayManager {
    pub tray_icon: TrayIcon,
}


include!("tray_manager_impl.rs");
