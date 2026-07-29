#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use blade_controlhub::config::ThemeColor;
use blade_controlhub::ipc::client;
use blade_controlhub::runtime::settings_state::SettingsState;
use blade_controlhub::ui::settings::Settings;
use blade_controlhub::ui::settings::SettingsCommand;
use blade_controlhub::ui::settings::store::SettingsStore;
use blade_controlhub::ui::theme::{
    SETTINGS_PADDING_RATIO, SETTINGS_WINDOW_SIZE, SETTINGS_WINDOW_TITLE,
};
use eframe::egui;
use std::time::{Duration, Instant};
use tracing::warn;

const SETTINGS_FRAME_INTERVAL: Duration = Duration::from_millis(16);
const SETTINGS_STATE_RETRY_INTERVAL: Duration = Duration::from_millis(500);

struct SettingsApp {
    settings: SettingsStore,
    state_loaded: bool,
    last_state_refresh: Option<Instant>,
    last_frame_at: Option<Instant>,
    applied_window_icon_color: Option<ThemeColor>,
    native_window_icons: Option<NativeWindowIcons>,
}

impl SettingsApp {
    fn new(state: Option<SettingsState>) -> Self {
        let settings = SettingsStore::new();
        settings.show(state.clone().unwrap_or_default());
        Self {
            settings,
            state_loaded: state.is_some(),
            last_state_refresh: Some(Instant::now()),
            last_frame_at: None,
            applied_window_icon_color: None,
            native_window_icons: None,
        }
    }

    fn process_backend(&mut self, ctx: &egui::Context) {
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
                SettingsCommand::BeginRazerKeyCapture => {
                    if let Err(error) = client::begin_razer_key_capture() {
                        warn!(%error, "Failed to start Razer key capture");
                    }
                }
                SettingsCommand::CancelRazerKeyCapture => {
                    if let Err(error) = client::cancel_razer_key_capture() {
                        warn!(%error, "Failed to cancel Razer key capture");
                    }
                }
            }
        }

        let is_listening = self
            .settings
            .with_settings(|settings| settings.custom_key_map.get_listening_idx().is_some());
        if is_listening {
            match client::poll_captured_razer_key() {
                Ok(Some(key_code)) => {
                    self.settings
                        .with_settings(|settings| settings.apply_captured_razer_key(key_code));
                }
                Ok(None) => {}
                Err(error) => warn!(%error, "Failed to poll captured Razer key"),
            }
        }

        if command_sent || self.should_refresh_state() {
            self.refresh_settings_state(ctx);
        } else if let Some(retry_after) = self.next_state_retry_after() {
            ctx.request_repaint_after(retry_after);
        }
    }

    fn should_refresh_state(&self) -> bool {
        !self.state_loaded
            && self
                .last_state_refresh
                .is_none_or(|last_refresh| last_refresh.elapsed() >= SETTINGS_STATE_RETRY_INTERVAL)
    }

    fn next_state_retry_after(&self) -> Option<Duration> {
        if self.state_loaded {
            return None;
        }

        Some(
            self.last_state_refresh
                .map(|last_refresh| {
                    SETTINGS_STATE_RETRY_INTERVAL.saturating_sub(last_refresh.elapsed())
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
        } else if let Some(retry_after) = self.next_state_retry_after() {
            ctx.request_repaint_after(retry_after);
        }
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
                .unwrap_or_default()
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

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.pace_frame();
        self.process_backend(ctx);
        self.settings.with_settings(|settings| settings.ui(ctx));
        self.update_window_icon(frame);
    }
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
    match client::get_settings_state() {
        Ok(state) => Some(state),
        Err(error) => {
            warn!(%error, "Failed to load settings state from Blade ControlHub runtime");
            None
        }
    }
}

fn main() -> eframe::Result<()> {
    let initial_state = try_load_settings_state();
    let icon_data = Settings::load_settings_icon(
        initial_state
            .as_ref()
            .map(|state| state.theme_color)
            .unwrap_or_default(),
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

    if let Some(monitor) = pre_resolve_monitor_dimensions() {
        viewport = viewport.with_position(settings_spawn_position(monitor, window_size));
    }

    native_options.viewport = viewport;

    eframe::run_native(
        SETTINGS_WINDOW_TITLE,
        native_options,
        Box::new(move |_cc| Box::new(SettingsApp::new(initial_state))),
    )
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
