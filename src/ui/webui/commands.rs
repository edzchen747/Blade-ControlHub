//! Everything the settings window is allowed to ask the runtime to do.
//!
//! Each command is a thin adapter over [`DeviceHandle`], which serialises work
//! onto the single thread that owns the HID device. Device calls can block for
//! up to the handle's timeout, so every one of them is moved off Tauri's event
//! loop with `spawn_blocking`; a slow device must never freeze the window or
//! the tray.
//!
//! A command that succeeds notifies the push worker, which is how the window
//! learns the authoritative value after its optimistic update.

use tauri::ipc::Invoke;
use tracing::warn;

use crate::config::ThemeColor;
use crate::error::AppResult;
use crate::razer::config::{CustomToggle, HiddenDashboardControls, PowerProfile};
use crate::razer::device_handle::{DeviceHandle, device};
use crate::razer::enums::{BatteryLimit, PerfMode, RGBEffect};
use crate::win::input::binding::KeyBindings;
use crate::win::system::app_index::{self, IndexedApp};
use crate::win::system::file_picker;
use crate::win::system::usbpcap::capture::CapturedCommand;
use crate::win::system::usbpcap::{USBPCAP_DOWNLOAD_URL, UsbpcapStatus, usbpcap_driver_label};

use super::command_lab::{self, CommandLabRecordingState};
use super::key_capture;
use super::push;
use super::state::UiState;

type CommandResult<T> = Result<T, String>;

pub fn handler() -> impl Fn(Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        get_state,
        set_perf_mode,
        set_custom_mode_config,
        set_fan_speed,
        set_refresh_rate,
        set_keyboard_brightness,
        set_rgb_effect,
        set_under_glow,
        set_battery_limit,
        set_primary_multimedia_keys,
        set_advanced_experimental_features,
        set_start_with_admin,
        set_start_with_windows,
        set_theme_color,
        begin_razer_key_capture,
        cancel_razer_key_capture,
        forward_key,
        set_key_bindings,
        list_apps,
        refresh_apps,
        pick_executable,
        begin_command_lab_record,
        cancel_command_lab_record,
        get_command_lab_state,
        play_command_lab_commands,
        save_command_lab_commands,
        remove_command_lab_command,
        save_custom_toggles,
        set_custom_toggle,
        set_hidden_dashboard_controls,
        get_usbpcap_status,
        close_gpu_apps,
        get_app_theme,
        hide_window,
        restart_app,
        quit_app
    ]
}

// ── State ────────────────────────────────────────────────────────────────────

#[tauri::command]
async fn get_state() -> CommandResult<UiState> {
    on_device(|device| device.get_settings_state())
        .await
        .map(UiState::new)
}

// ── Performance ──────────────────────────────────────────────────────────────

#[tauri::command]
async fn set_perf_mode(profile: PowerProfile, mode: PerfMode) -> CommandResult<()> {
    changed(on_device(move |device| device.set_perf_mode(profile, mode)).await)
}

#[tauri::command]
async fn set_custom_mode_config(cpu_level: u8, gpu_level: u8) -> CommandResult<()> {
    changed(on_device(move |device| device.set_custom_mode_config(cpu_level, gpu_level)).await)
}

#[tauri::command]
async fn set_fan_speed(profile: PowerProfile, speed: u8) -> CommandResult<()> {
    changed(on_device(move |device| device.set_fan_speed(profile, speed)).await)
}

// ── Display ──────────────────────────────────────────────────────────────────

#[tauri::command]
async fn set_refresh_rate(profile: PowerProfile, hz: u32) -> CommandResult<()> {
    changed(on_device(move |device| device.set_refresh_rate(profile, hz)).await)
}

// ── Lighting ─────────────────────────────────────────────────────────────────

#[tauri::command]
async fn set_keyboard_brightness(profile: PowerProfile, level: u8) -> CommandResult<()> {
    changed(on_device(move |device| device.set_keyboard_brightness(profile, level)).await)
}

#[tauri::command]
async fn set_rgb_effect(profile: PowerProfile, effect: RGBEffect) -> CommandResult<()> {
    changed(on_device(move |device| device.set_rgb_mode(profile, effect)).await)
}

#[tauri::command]
async fn set_under_glow(profile: PowerProfile, enabled: bool) -> CommandResult<()> {
    changed(on_device(move |device| device.set_under_glow(profile, enabled)).await)
}

// ── System ───────────────────────────────────────────────────────────────────

#[tauri::command]
async fn set_battery_limit(limit: BatteryLimit) -> CommandResult<()> {
    changed(on_device(move |device| device.set_battery_limit(limit)).await)
}

#[tauri::command]
async fn set_primary_multimedia_keys(enabled: bool) -> CommandResult<()> {
    changed(on_device(move |device| device.set_primary_multimedia_keys(enabled)).await)
}

#[tauri::command]
async fn set_advanced_experimental_features(enabled: bool) -> CommandResult<()> {
    changed(on_device(move |device| device.set_advanced_experimental_features(enabled)).await)
}

/// Turning this on relaunches the app elevated; the current process shuts down
/// only once the replacement has started. Turning it off applies on the next
/// manual launch and leaves this session's privileges alone.
#[tauri::command]
async fn set_start_with_admin(enabled: bool) -> CommandResult<()> {
    let result = on_device(move |device| device.set_start_with_admin(enabled)).await;
    if result.is_ok() {
        crate::utils::reload::relaunch_after_admin_toggle(enabled);
    }
    changed(result)
}

#[tauri::command]
async fn set_start_with_windows(enabled: bool) -> CommandResult<()> {
    changed(
        on_device(move |device| {
            device.set_start_with_windows(enabled);
            Ok(())
        })
        .await,
    )
}

#[tauri::command]
async fn set_theme_color(color: ThemeColor) -> CommandResult<()> {
    changed(on_device(move |device| device.set_theme_color(color)).await)
}

// ── Key mapping ──────────────────────────────────────────────────────────────

#[tauri::command]
fn begin_razer_key_capture() {
    key_capture::begin_razer_key_capture();
}

#[tauri::command]
fn cancel_razer_key_capture() {
    key_capture::stop_razer_key_capture();
}

/// Runs a key the settings window saw, because the runtime's keyboard hook did
/// not: Windows does not invoke a low-level hook for input aimed at our own
/// window. Returns whether the runtime consumed it, so the window can report
/// whether the keystroke should have been swallowed.
///
/// Cheap and synchronous: the dispatch only reads atomics and hands any real
/// work to a worker, so it never blocks Tauri's event loop.
#[tauri::command]
fn forward_key(key_code: u8, pressed: bool) -> bool {
    crate::win::input::key_hook::dispatch_forwarded_key(key_code, pressed)
}

/// Replaces both mapping tables at once. The window owns the rows — including
/// the half-finished ones it needs to render — so it sends the whole set rather
/// than a diff; the runtime drops the incomplete rows when it installs them.
#[tauri::command]
async fn set_key_bindings(bindings: KeyBindings) -> CommandResult<()> {
    changed(
        on_device(move |device| {
            device.set_key_bindings(bindings);
            Ok(())
        })
        .await,
    )
}

/// The Start menu index, built on first use. The first call can take a second
/// or two on a machine with a large Start menu, so it never runs on the event
/// loop.
#[tauri::command]
async fn list_apps() -> CommandResult<Vec<IndexedApp>> {
    blocking(app_index::list).await
}

#[tauri::command]
async fn refresh_apps() -> CommandResult<Vec<IndexedApp>> {
    blocking(app_index::refresh).await
}

/// Opens the system file picker, owned by the settings window so it cannot be
/// lost behind it. `None` means the user cancelled.
#[tauri::command]
async fn pick_executable() -> CommandResult<Option<String>> {
    let owner = super::window::raw_handle();
    blocking(move || {
        file_picker::pick_executable(owner).map(|path| path.to_string_lossy().into_owned())
    })
    .await
}

// ── Command Lab ──────────────────────────────────────────────────────────────

/// Resolves once the USB capture is actually running, which is after any UAC
/// prompt has been answered. Progress after that arrives as `command-lab`
/// events.
#[tauri::command]
async fn begin_command_lab_record() -> CommandResult<CommandLabRecordingState> {
    blocking(command_lab::begin_command_lab_record).await
}

#[tauri::command]
async fn cancel_command_lab_record() -> CommandResult<()> {
    blocking(command_lab::cancel_command_lab_record).await
}

#[tauri::command]
fn get_command_lab_state() -> CommandLabRecordingState {
    command_lab::command_lab_state()
}

#[tauri::command]
async fn play_command_lab_commands(commands: Vec<CapturedCommand>) -> CommandResult<()> {
    on_device(move |device| {
        device.play_command_lab_commands(commands);
        Ok(())
    })
    .await
}

#[tauri::command]
async fn save_command_lab_commands(
    name: String,
    commands: Vec<CapturedCommand>,
) -> CommandResult<()> {
    changed(
        on_device(move |device| {
            device.save_command_lab_commands(name, commands);
            Ok(())
        })
        .await,
    )
}

#[tauri::command]
async fn remove_command_lab_command(name: String) -> CommandResult<()> {
    changed(
        on_device(move |device| {
            device.remove_command_lab_command(name);
            Ok(())
        })
        .await,
    )
}

#[tauri::command]
async fn save_custom_toggles(toggles: Vec<CustomToggle>) -> CommandResult<()> {
    changed(
        on_device(move |device| {
            device.set_custom_toggles(toggles);
            Ok(())
        })
        .await,
    )
}

#[tauri::command]
async fn set_custom_toggle(
    profile: PowerProfile,
    name: String,
    enabled: bool,
) -> CommandResult<()> {
    changed(
        on_device(move |device| {
            device.set_custom_toggle(profile, name, enabled);
            Ok(())
        })
        .await,
    )
}

#[tauri::command]
async fn set_hidden_dashboard_controls(hidden: HiddenDashboardControls) -> CommandResult<()> {
    changed(
        on_device(move |device| {
            device.set_hidden_dashboard_controls(hidden);
            Ok(())
        })
        .await,
    )
}

#[derive(serde::Serialize)]
struct UsbpcapInfo {
    installed: bool,
    available_interfaces: u32,
    label: String,
    download_url: &'static str,
}

#[tauri::command]
async fn get_usbpcap_status() -> CommandResult<UsbpcapInfo> {
    blocking(|| {
        let status = crate::win::system::usbpcap::usbpcap_status();
        UsbpcapInfo {
            installed: matches!(status, UsbpcapStatus::Installed { .. }),
            available_interfaces: match status {
                UsbpcapStatus::Installed {
                    available_interfaces,
                } => available_interfaces,
                UsbpcapStatus::NotInstalled => 0,
            },
            label: usbpcap_driver_label(status),
            download_url: USBPCAP_DOWNLOAD_URL,
        }
    })
    .await
}

// ── App lifecycle ────────────────────────────────────────────────────────────

#[tauri::command]
async fn close_gpu_apps() -> CommandResult<()> {
    blocking(crate::win::system::cli_utils::cycle_gpu).await
}

/// The Windows app mode, for the window's first paint. Changes after that
/// arrive as the `app-theme` event.
#[tauri::command]
fn get_app_theme() -> crate::win::system::app_theme::AppTheme {
    super::app_theme::current()
}

#[tauri::command]
fn hide_window() {
    super::window::hide_window();
}

#[tauri::command]
fn restart_app() {
    spawn_app_event("restart", crate::ui::app_events::AppEvent::Restart(0));
}

#[tauri::command]
fn quit_app() {
    spawn_app_event("quit", crate::ui::app_events::AppEvent::Shutdown);
}

// ── Plumbing ─────────────────────────────────────────────────────────────────

/// Runs a device call on a blocking worker and maps its error to a message the
/// window can show next to the control that failed.
async fn on_device<T, F>(work: F) -> CommandResult<T>
where
    F: FnOnce(&DeviceHandle) -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    let result = tauri::async_runtime::spawn_blocking(move || work(&device()))
        .await
        .map_err(|error| format!("command worker failed: {error}"))?;

    result.map_err(|error| error.to_string())
}

async fn blocking<T, F>(work: F) -> CommandResult<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| format!("command worker failed: {error}"))
}

/// Marks runtime state as stale after a successful command so the window is
/// reconciled against the device rather than its own optimistic guess.
fn changed<T>(result: CommandResult<T>) -> CommandResult<T> {
    if result.is_ok() {
        push::notify_settings_changed();
    }
    result
}

/// Shutdown and restart tear down the Tauri runtime, so they cannot run on its
/// own event loop.
fn spawn_app_event(name: &str, event: crate::ui::app_events::AppEvent) {
    if let Err(error) = std::thread::Builder::new()
        .name(format!("blade-window-{name}"))
        .spawn(move || crate::ui::app::app(event))
    {
        warn!(%error, name, "Failed to spawn the app lifecycle worker");
    }
}
