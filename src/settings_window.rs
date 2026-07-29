#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use blade_controlhub::config::ThemeColor;
use blade_controlhub::ipc::client;
use blade_controlhub::razer::enums::PerfMode;
use blade_controlhub::runtime::settings_state::SettingsState;
use blade_controlhub::ui::settings::Settings;
use blade_controlhub::ui::settings::SettingsCommand;
use blade_controlhub::ui::settings::store::SettingsStore;
use blade_controlhub::ui::theme::{
    SETTINGS_KEY_LISTEN_INTERVAL_MS, SETTINGS_LOADING_ICON_COLOR, SETTINGS_PADDING_RATIO,
    SETTINGS_WINDOW_SIZE, SETTINGS_WINDOW_TITLE,
};
use blade_controlhub::utils::log_file::{init_log_file_writer_for_child, set_cwd};
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
const SETTINGS_STATE_REFRESH_INTERVAL: Duration = Duration::from_millis(500);

struct SettingsApp {
    settings: SettingsStore,
    state_loaded: bool,
    last_state_refresh: Option<Instant>,
    last_frame_at: Option<Instant>,
    razer_key_capture_tx: Sender<RazerKeyCaptureMessage>,
    razer_key_capture_rx: Receiver<RazerKeyCaptureMessage>,
    razer_key_capture_cancel: Option<Arc<AtomicBool>>,
    razer_key_capture_id: u64,
    active_razer_key_capture_id: Option<u64>,
    applied_window_icon_color: Option<ThemeColor>,
    native_window_icons: Option<NativeWindowIcons>,
}

impl SettingsApp {
    fn new(state: Option<SettingsState>) -> Self {
        let settings = SettingsStore::new();
        let (razer_key_capture_tx, razer_key_capture_rx) = mpsc::channel();
        if let Some(state) = state.clone() {
            settings.show(state);
        } else {
            settings.with_settings(|settings| {
                settings.show = true;
                settings.update = true;
                settings.state = None;
            });
        }
        Self {
            settings,
            state_loaded: state.is_some(),
            last_state_refresh: Some(Instant::now()),
            last_frame_at: None,
            razer_key_capture_tx,
            razer_key_capture_rx,
            razer_key_capture_cancel: None,
            razer_key_capture_id: 0,
            active_razer_key_capture_id: None,
            applied_window_icon_color: None,
            native_window_icons: None,
        }
    }

    fn process_backend(&mut self, ctx: &egui::Context) {
        self.drain_razer_key_capture_messages(ctx);

        let commands = self
            .settings
            .with_settings(|settings| settings.drain_commands());
        let mut command_sent = false;

        for command in commands {
            match command {
                SettingsCommand::SetDefaultMultimediaKeys(enabled) => {
                    if let Err(error) = client::set_default_multimedia_keys(enabled) {
                        warn!(%error, "Failed to update default multimedia key setting");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetPerfMode(profile, mode) => {
                    if let Err(error) = client::set_perf_mode(profile, mode) {
                        warn!(%error, "Failed to update performance mode");
                        self.handle_failed_perf_mode_update(mode, ctx);
                        command_sent = true;
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetRefreshRate(profile, hz) => {
                    if let Err(error) = client::set_refresh_rate(profile, hz) {
                        warn!(%error, "Failed to update refresh rate");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetKeyboardBrightness(profile, level) => {
                    if let Err(error) = client::set_keyboard_brightness(profile, level) {
                        warn!(%error, "Failed to update keyboard brightness");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetRgbEffect(profile, effect) => {
                    if let Err(error) = client::set_rgb_effect(profile, effect) {
                        warn!(%error, "Failed to update RGB effect");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetUnderGlow(profile, enabled) => {
                    if let Err(error) = client::set_under_glow(profile, enabled) {
                        warn!(%error, "Failed to update vapour chamber light");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetBatteryLimit(limit) => {
                    if let Err(error) = client::set_battery_limit(limit) {
                        warn!(%error, "Failed to update battery charge limit");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::SetThemeColor(color) => {
                    if let Err(error) = client::set_theme_color(color) {
                        warn!(%error, "Failed to update theme color");
                    } else {
                        command_sent = true;
                    }
                }
                SettingsCommand::BeginRazerKeyCapture { row_idx } => {
                    self.start_razer_key_capture(row_idx, ctx);
                }
                SettingsCommand::CancelRazerKeyCapture => {
                    self.cancel_razer_key_capture(ctx);
                }
            }
        }

        if command_sent || self.should_refresh_state() {
            self.refresh_settings_state(ctx);
        } else if let Some(refresh_after) = self.next_state_refresh_after() {
            ctx.request_repaint_after(refresh_after);
        }
    }

    fn start_razer_key_capture(&mut self, row_idx: usize, ctx: &egui::Context) {
        if let Some(cancel) = self.razer_key_capture_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }

        self.razer_key_capture_id = self.razer_key_capture_id.saturating_add(1);
        let capture_id = self.razer_key_capture_id;
        self.active_razer_key_capture_id = Some(capture_id);
        let after_unix_ms = current_unix_ms();
        self.settings.with_settings(|settings| {
            settings.custom_key_map.special_key = None;
            settings.custom_key_map.set_listening_idx(Some(row_idx));
        });
        ctx.request_repaint();

        let cancel = Arc::new(AtomicBool::new(false));
        self.razer_key_capture_cancel = Some(cancel.clone());
        let tx = self.razer_key_capture_tx.clone();
        let worker_ctx = ctx.clone();

        if let Err(error) = thread::Builder::new()
            .name("blade-settings-razer-key-capture".to_string())
            .spawn(move || {
                run_razer_key_capture_worker(capture_id, after_unix_ms, cancel, tx, worker_ctx)
            })
        {
            warn!(%error, "Failed to spawn Razer key capture worker");
            self.razer_key_capture_cancel = None;
            self.active_razer_key_capture_id = None;
            self.settings
                .with_settings(|settings| settings.custom_key_map.set_listening_idx(None));
            ctx.request_repaint();
        }
    }

    fn cancel_razer_key_capture(&mut self, ctx: &egui::Context) {
        if let Some(cancel) = self.razer_key_capture_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.active_razer_key_capture_id = None;
        self.settings
            .with_settings(|settings| settings.custom_key_map.set_listening_idx(None));
        ctx.request_repaint();

        let ctx = ctx.clone();
        if let Err(error) = thread::Builder::new()
            .name("blade-settings-razer-key-cancel".to_string())
            .spawn(move || {
                if let Err(error) = client::cancel_razer_key_capture() {
                    warn!(%error, "Failed to cancel Razer key capture");
                }
                ctx.request_repaint();
            })
        {
            warn!(%error, "Failed to spawn Razer key capture cancel worker");
        }
    }

    fn drain_razer_key_capture_messages(&mut self, ctx: &egui::Context) {
        while let Ok(message) = self.razer_key_capture_rx.try_recv() {
            if Some(message.capture_id()) != self.active_razer_key_capture_id {
                continue;
            }

            match message {
                RazerKeyCaptureMessage::Captured {
                    capture_id: _,
                    key_code,
                } => {
                    self.razer_key_capture_cancel = None;
                    self.active_razer_key_capture_id = None;
                    self.settings
                        .with_settings(|settings| settings.apply_captured_razer_key(key_code));
                    ctx.request_repaint();
                }
            }
        }
    }

    fn should_refresh_state(&self) -> bool {
        self.last_state_refresh
            .is_none_or(|last_refresh| last_refresh.elapsed() >= SETTINGS_STATE_REFRESH_INTERVAL)
    }

    fn next_state_refresh_after(&self) -> Option<Duration> {
        Some(
            self.last_state_refresh
                .map(|last_refresh| {
                    SETTINGS_STATE_REFRESH_INTERVAL.saturating_sub(last_refresh.elapsed())
                })
                .unwrap_or(Duration::ZERO),
        )
    }

    fn refresh_settings_state(&mut self, ctx: &egui::Context) {
        self.last_state_refresh = Some(Instant::now());
        if let Some(state) = try_load_settings_state() {
            self.settings.update_state(state);
            self.state_loaded = true;
            ctx.request_repaint();
        } else if let Some(refresh_after) = self.next_state_refresh_after() {
            ctx.request_repaint_after(refresh_after);
        }
    }

    fn handle_failed_perf_mode_update(&mut self, mode: PerfMode, ctx: &egui::Context) {
        self.settings.with_settings(|settings| {
            settings.flash_unsupported_perf_mode(mode);
        });
        ctx.request_repaint();
        ctx.request_repaint_after(Settings::unsupported_perf_mode_notice_duration());
    }

    fn pace_frame(&mut self) {
        let now = Instant::now();
        if let Some(last_frame_at) = self.last_frame_at {
            let elapsed = now.saturating_duration_since(last_frame_at);
            if elapsed < SETTINGS_FRAME_INTERVAL {
                std::thread::sleep(SETTINGS_FRAME_INTERVAL - elapsed);
            }
        }
        self.last_frame_at = Some(Instant::now());
    }

    fn update_window_icon(&mut self, frame: &eframe::Frame) {
        let color = self.settings.with_settings(|settings| {
            settings
                .state
                .as_ref()
                .map(|state| state.theme_color)
                .unwrap_or(SETTINGS_LOADING_ICON_COLOR)
        });

        if self.applied_window_icon_color == Some(color) {
            return;
        }

        if let Some(icons) = NativeWindowIcons::apply(frame, color) {
            self.native_window_icons = Some(icons);
            self.applied_window_icon_color = Some(color);
        }
    }
}

impl Drop for SettingsApp {
    fn drop(&mut self) {
        if let Some(cancel) = self.razer_key_capture_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
            let _ = client::cancel_razer_key_capture();
        }
    }
}

enum RazerKeyCaptureMessage {
    Captured { capture_id: u64, key_code: u8 },
}

impl RazerKeyCaptureMessage {
    fn capture_id(&self) -> u64 {
        match self {
            Self::Captured { capture_id, .. } => *capture_id,
        }
    }
}

fn run_razer_key_capture_worker(
    capture_id: u64,
    after_unix_ms: u64,
    cancel: Arc<AtomicBool>,
    tx: Sender<RazerKeyCaptureMessage>,
    ctx: egui::Context,
) {
    let mut after_sequence = loop {
        if cancel.load(Ordering::SeqCst) {
            return;
        }

        match client::begin_razer_key_capture(after_unix_ms) {
            Ok(after_sequence) => break after_sequence,
            Err(error) => {
                warn!(%error, "Razer key capture begin failed; retrying");
                thread::sleep(razer_key_capture_retry_interval());
            }
        }
    };

    while !cancel.load(Ordering::SeqCst) {
        match client::poll_captured_razer_key(after_sequence) {
            Ok(Some(event)) => {
                after_sequence = event.sequence;
                if event.key_code == 0 {
                    continue;
                }

                let _ = tx.send(RazerKeyCaptureMessage::Captured {
                    capture_id,
                    key_code: event.key_code,
                });
                ctx.request_repaint();
                return;
            }
            Ok(None) => thread::sleep(razer_key_capture_poll_interval()),
            Err(error) => {
                warn!(%error, "Razer key capture poll failed; retrying");
                thread::sleep(razer_key_capture_retry_interval());
            }
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.pace_frame();
        self.settings.with_settings(|settings| settings.ui(ctx));
        self.process_backend(ctx);
        self.update_window_icon(frame);
    }
}

fn razer_key_capture_poll_interval() -> Duration {
    Duration::from_millis(SETTINGS_KEY_LISTEN_INTERVAL_MS)
}

fn razer_key_capture_retry_interval() -> Duration {
    Duration::from_millis(100)
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(target_os = "windows")]
struct NativeWindowIcons {
    big: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
    small: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
}

#[cfg(target_os = "windows")]
impl NativeWindowIcons {
    fn apply(frame: &eframe::Frame, color: ThemeColor) -> Option<Self> {
        use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, ICON_BIG, ICON_SMALL, SM_CXICON, SM_CXSMICON, SendMessageW,
            WM_SETICON,
        };

        let hwnd = match frame.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => handle.hwnd.get(),
            _ => return None,
        };

        let big_size = unsafe { GetSystemMetrics(SM_CXICON) }.max(1) as u32;
        let small_size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(1) as u32;
        let big = create_settings_hicon(color, big_size)?;
        let small = create_settings_hicon(color, small_size)?;

        unsafe {
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, big);
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small);
        }

        Some(Self { big, small })
    }
}

#[cfg(target_os = "windows")]
impl Drop for NativeWindowIcons {
    fn drop(&mut self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

        unsafe {
            DestroyIcon(self.big);
            DestroyIcon(self.small);
        }
    }
}

#[cfg(not(target_os = "windows"))]
struct NativeWindowIcons;

#[cfg(not(target_os = "windows"))]
impl NativeWindowIcons {
    fn apply(_frame: &eframe::Frame, _color: ThemeColor) -> Option<Self> {
        None
    }
}

#[cfg(target_os = "windows")]
fn create_settings_hicon(
    color: ThemeColor,
    size: u32,
) -> Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON> {
    use windows_sys::Win32::UI::WindowsAndMessaging::CreateIcon;

    let icon = Settings::load_settings_icon_with_size(color, size);
    let (and_mask, bgra) = windows_icon_masks_and_bgra(icon.rgba, icon.width, icon.height)?;

    let handle = unsafe {
        CreateIcon(
            0,
            icon.width as i32,
            icon.height as i32,
            1,
            32,
            and_mask.as_ptr(),
            bgra.as_ptr(),
        )
    };

    (handle != 0).then_some(handle)
}

#[cfg(target_os = "windows")]
fn windows_icon_masks_and_bgra(
    mut rgba: Vec<u8>,
    width: u32,
    height: u32,
) -> Option<(Vec<u8>, Vec<u8>)> {
    let pixel_count = width.checked_mul(height)? as usize;
    if rgba.len() != pixel_count.checked_mul(4)? {
        return None;
    }

    let mut and_mask = Vec::with_capacity(pixel_count);
    for pixel in rgba.chunks_exact_mut(4) {
        and_mask.push(pixel[3].wrapping_sub(u8::MAX));
        pixel.swap(0, 2);
    }

    Some((and_mask, rgba))
}

fn try_load_settings_state() -> Option<SettingsState> {
    let started = Instant::now();
    debug!("Loading settings state from Blade ControlHub runtime");
    match client::get_settings_state() {
        Ok(state) => {
            info!(
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Loaded settings state from Blade ControlHub runtime"
            );
            Some(state)
        }
        Err(error) => {
            warn!(
                %error,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Failed to load settings state from Blade ControlHub runtime"
            );
            None
        }
    }
}

fn main() -> eframe::Result<()> {
    let process_started = Instant::now();
    if let Err(error) = set_cwd() {
        eprintln!("Failed to set settings window working directory: {error}");
    }
    init_log_file_writer_for_child("Start Settings Window Session");
    info!("Settings window process started");

    let initial_state = try_load_settings_state();
    info!(
        state_loaded = initial_state.is_some(),
        elapsed_ms = process_started.elapsed().as_millis() as u64,
        "Settings window initial state load completed"
    );

    let icon_started = Instant::now();
    let icon_data = Settings::load_settings_icon(
        initial_state
            .as_ref()
            .map(|state| state.theme_color)
            .unwrap_or(SETTINGS_LOADING_ICON_COLOR),
    );
    debug!(
        elapsed_ms = icon_started.elapsed().as_millis() as u64,
        "Loaded settings window icon"
    );
    let window_size = SETTINGS_WINDOW_SIZE;

    let mut native_options = eframe::NativeOptions::default();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(SETTINGS_WINDOW_TITLE)
        .with_icon(icon_data)
        .with_inner_size(window_size)
        .with_min_inner_size(window_size)
        .with_max_inner_size(window_size)
        .with_resizable(false)
        .with_maximize_button(false);

    let monitor_started = Instant::now();
    if let Some(monitor) = pre_resolve_monitor_dimensions() {
        debug!(
            elapsed_ms = monitor_started.elapsed().as_millis() as u64,
            "Resolved settings window spawn position"
        );
        viewport = viewport.with_position(settings_spawn_position(monitor, window_size));
    } else {
        debug!(
            elapsed_ms = monitor_started.elapsed().as_millis() as u64,
            "Settings window spawn position was not pre-resolved"
        );
    }

    native_options.viewport = viewport;

    info!(
        elapsed_ms = process_started.elapsed().as_millis() as u64,
        "Starting settings native window"
    );
    let result = eframe::run_native(
        SETTINGS_WINDOW_TITLE,
        native_options,
        Box::new(move |_cc| Box::new(SettingsApp::new(initial_state))),
    );

    match &result {
        Ok(()) => info!(
            elapsed_ms = process_started.elapsed().as_millis() as u64,
            "Settings native window exited"
        ),
        Err(error) => warn!(
            error = ?error,
            elapsed_ms = process_started.elapsed().as_millis() as u64,
            "Settings native window failed"
        ),
    }

    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SimpleMonitor {
    width: u32,
    height: u32,
}

fn settings_spawn_position(monitor: SimpleMonitor, window_size: egui::Vec2) -> egui::Pos2 {
    let screen_width = monitor.width as f32;
    let screen_height = monitor.height as f32;
    let padding = screen_height * SETTINGS_PADDING_RATIO;

    egui::pos2(
        screen_width - window_size.x - padding * 0.1,
        screen_height - window_size.y - padding,
    )
}

fn pre_resolve_monitor_dimensions() -> Option<SimpleMonitor> {
    let (width, height) = primary_screen_dimensions();
    monitor_from_dimensions(width, height)
}

fn monitor_from_dimensions(width: i32, height: i32) -> Option<SimpleMonitor> {
    if width > 0 && height > 0 {
        Some(SimpleMonitor {
            width: width as u32,
            height: height as u32,
        })
    } else {
        None
    }
}

fn primary_screen_dimensions() -> (i32, i32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    // SAFETY: GetSystemMetrics is a side-effect-free Win32 query for process
    // desktop metrics and accepts these constant indexes.
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_spawn_position_uses_bottom_right_padding() {
        let monitor = SimpleMonitor {
            width: 1920,
            height: 1080,
        };
        let position = settings_spawn_position(monitor, egui::vec2(453.0, 631.0));

        assert_eq!(
            position,
            egui::pos2(
                1920.0 - 453.0 - (1080.0 * SETTINGS_PADDING_RATIO * 0.1),
                1080.0 - 631.0 - (1080.0 * SETTINGS_PADDING_RATIO),
            )
        );
    }

    #[test]
    fn invalid_monitor_dimensions_are_ignored() {
        assert_eq!(monitor_from_dimensions(0, 1080), None);
        assert_eq!(monitor_from_dimensions(1920, 0), None);
        assert_eq!(monitor_from_dimensions(-1, 1080), None);
    }

    #[test]
    fn settings_app_starts_loading_without_default_settings_state() {
        let app = SettingsApp::new(None);

        assert!(!app.state_loaded);
        assert!(
            app.settings
                .with_settings(|settings| settings.state.is_none() && settings.show)
        );
    }

    #[test]
    fn settings_app_refreshes_state_periodically_after_initial_load() {
        let mut app = SettingsApp::new(Some(SettingsState::default()));
        app.last_state_refresh = Some(Instant::now() - SETTINGS_STATE_REFRESH_INTERVAL);

        assert!(app.should_refresh_state());
    }

    #[test]
    fn settings_app_schedules_next_periodic_state_refresh_after_initial_load() {
        let app = SettingsApp::new(Some(SettingsState::default()));

        assert!(app.next_state_refresh_after().is_some());
    }

    #[test]
    fn failed_perf_mode_update_flashes_unsupported_notice() {
        let mut app = SettingsApp::new(Some(SettingsState::default()));
        let ctx = egui::Context::default();

        app.handle_failed_perf_mode_update(PerfMode::Performance, &ctx);

        assert_eq!(
            app.settings
                .with_settings(|settings| settings.unsupported_perf_mode_message()),
            Some("\"Performance\" mode not supported on device".to_string())
        );
    }

    #[test]
    fn razer_key_capture_poll_interval_uses_settings_listen_interval() {
        assert_eq!(
            razer_key_capture_poll_interval(),
            Duration::from_millis(SETTINGS_KEY_LISTEN_INTERVAL_MS)
        );
    }

    #[test]
    fn positive_monitor_dimensions_are_preserved() {
        assert_eq!(
            monitor_from_dimensions(1920, 1080),
            Some(SimpleMonitor {
                width: 1920,
                height: 1080,
            })
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_icon_conversion_builds_alpha_mask_and_bgra_pixels() {
        let rgba = vec![
            0x11, 0x22, 0x33, 0xff, //
            0xaa, 0xbb, 0xcc, 0x00,
        ];

        let (and_mask, bgra) =
            windows_icon_masks_and_bgra(rgba, 2, 1).expect("valid icon data should convert");

        assert_eq!(and_mask, vec![0x00, 0x01]);
        assert_eq!(
            bgra,
            vec![
                0x33, 0x22, 0x11, 0xff, //
                0xcc, 0xbb, 0xaa, 0x00,
            ]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_icon_conversion_rejects_wrong_buffer_length() {
        assert!(windows_icon_masks_and_bgra(vec![0xff, 0x00, 0x00], 1, 1).is_none());
    }
}
