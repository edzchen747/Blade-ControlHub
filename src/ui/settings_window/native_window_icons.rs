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

