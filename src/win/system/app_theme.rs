//! Follows the Windows app mode (Settings > Personalisation > Colours > "Choose
//! your default app mode") as it changes.
//!
//! Windows announces a mode change by broadcasting `WM_SETTINGCHANGE` with the
//! string "ImmersiveColorSet" to every top-level window, after it has written the
//! new `AppsUseLightTheme` value. A hidden top-level window is kept on its own
//! thread to receive that broadcast — a message-only window would not, because
//! broadcasts skip them — and the value is only read when it arrives, never
//! polled.

use std::mem::MaybeUninit;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread;

use serde::Serialize;
use tracing::{debug, info, warn};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, RegisterClassW,
    WM_SETTINGCHANGE, WNDCLASSW,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AppTheme {
    Light,
    Dark,
}

/// The `lParam` string Windows sends with `WM_SETTINGCHANGE` when the app or
/// system colour mode changes.
const COLOR_SET_CHANGED: &str = "ImmersiveColorSet";

type Listener = Box<dyn Fn(AppTheme) + Send + Sync>;
static LISTENER: OnceLock<Listener> = OnceLock::new();

/// The last theme handed to the listener, so the broadcast Windows also sends
/// for accent and taskbar changes does not re-apply an unchanged theme.
static LAST: AtomicU8 = AtomicU8::new(UNKNOWN);
const UNKNOWN: u8 = 0;

/// The app mode as Windows currently has it.
pub fn current() -> AppTheme {
    theme_from_apps_use_light_theme(read_apps_use_light_theme())
}

/// Calls `on_change` whenever the app mode flips, for the life of the process.
/// Only the first call installs a listener.
pub fn watch(on_change: impl Fn(AppTheme) + Send + Sync + 'static) {
    if LISTENER.set(Box::new(on_change)).is_err() {
        warn!("App theme watcher was already started");
        return;
    }
    LAST.store(encode(current()), Ordering::SeqCst);

    if let Err(error) = thread::Builder::new()
        .name("blade-app-theme".to_string())
        .spawn(run_message_loop)
    {
        warn!(%error, "Failed to start the app theme watcher; the window will not follow mode changes");
    }
}

fn run_message_loop() {
    let class_name = wide("BladeAppThemeWindow");
    unsafe {
        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wnd_class);

        // A plain top-level window, never shown. Not HWND_MESSAGE: message-only
        // windows are left out of broadcasts, which is the one thing it is for.
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            std::ptr::null(),
        );
        if hwnd == 0 {
            warn!("App theme window could not be created; the window will not follow mode changes");
            return;
        }

        loop {
            let mut msg = MaybeUninit::<MSG>::uninit();
            match GetMessageW(msg.as_mut_ptr(), 0, 0, 0) {
                value if value > 0 => {
                    // SAFETY: GetMessageW returned > 0, so it initialised `msg`.
                    DispatchMessageW(&msg.assume_init());
                }
                0 => break,
                _ => {
                    warn!("App theme message loop failed");
                    break;
                }
            }
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_SETTINGCHANGE && unsafe { setting_name(lparam) }.as_deref() == Some(COLOR_SET_CHANGED)
    {
        on_color_set_changed();
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn on_color_set_changed() {
    let theme = current();
    if LAST.swap(encode(theme), Ordering::SeqCst) == encode(theme) {
        debug!(?theme, "Colour settings changed without changing the app mode");
        return;
    }
    info!(?theme, "App mode changed");
    if let Some(listener) = LISTENER.get() {
        listener(theme);
    }
}

/// The setting name a `WM_SETTINGCHANGE` carries in `lParam`, if any.
///
/// # Safety
/// `lparam` must be the `lParam` of a `WM_SETTINGCHANGE` delivered to a Unicode
/// window: null, or a NUL-terminated UTF-16 string.
unsafe fn setting_name(lparam: LPARAM) -> Option<String> {
    let ptr = lparam as *const u16;
    if ptr.is_null() {
        return None;
    }
    let mut len = 0;
    // Bounded, so a malformed broadcast cannot walk off into memory.
    while len < 256 && unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    Some(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(ptr, len)
    }))
}

fn read_apps_use_light_theme() -> Option<u32> {
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let value = wide("AppsUseLightTheme");
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}

/// Windows ships in light mode and only writes the value once the user picks
/// one, so a missing value means light.
fn theme_from_apps_use_light_theme(value: Option<u32>) -> AppTheme {
    match value {
        Some(0) => AppTheme::Dark,
        _ => AppTheme::Light,
    }
}

fn encode(theme: AppTheme) -> u8 {
    match theme {
        AppTheme::Light => 1,
        AppTheme::Dark => 2,
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_means_dark_and_anything_else_light() {
        assert_eq!(theme_from_apps_use_light_theme(Some(0)), AppTheme::Dark);
        assert_eq!(theme_from_apps_use_light_theme(Some(1)), AppTheme::Light);
    }

    #[test]
    fn a_missing_value_is_the_windows_default_of_light() {
        assert_eq!(theme_from_apps_use_light_theme(None), AppTheme::Light);
    }

    #[test]
    fn reads_the_setting_name_windows_broadcasts() {
        let name = wide(COLOR_SET_CHANGED);
        let read = unsafe { setting_name(name.as_ptr() as LPARAM) };
        assert_eq!(read.as_deref(), Some(COLOR_SET_CHANGED));
    }

    #[test]
    fn a_settings_change_without_a_name_is_not_a_mode_change() {
        assert_eq!(unsafe { setting_name(0) }, None);
    }

    #[test]
    fn serialises_as_the_names_the_window_expects() {
        assert_eq!(serde_json::to_string(&AppTheme::Light).unwrap(), "\"light\"");
        assert_eq!(serde_json::to_string(&AppTheme::Dark).unwrap(), "\"dark\"");
    }
}
