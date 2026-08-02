struct DesktopDc {
    hdc: HDC,
}

impl DesktopDc {
    fn acquire() -> Option<Self> {
        let hdc = unsafe { GetDC(None) };
        (!hdc.0.is_null()).then_some(Self { hdc })
    }
}

impl Drop for DesktopDc {
    fn drop(&mut self) {
        let _ = unsafe { ReleaseDC(None, self.hdc) };
    }
}

struct CompatibleDc {
    hdc: HDC,
}

impl CompatibleDc {
    fn create(source: HDC) -> Option<Self> {
        let hdc = unsafe { CreateCompatibleDC(source) };
        (!hdc.0.is_null()).then_some(Self { hdc })
    }
}

impl Drop for CompatibleDc {
    fn drop(&mut self) {
        let _ = unsafe { DeleteDC(self.hdc) };
    }
}

struct OwnedBitmap {
    handle: HBITMAP,
}

impl OwnedBitmap {
    fn create_dib_section(
        hdc: HDC,
        bmi: &BITMAPINFO,
        bits: &mut *mut std::ffi::c_void,
    ) -> Option<Self> {
        let handle = unsafe { CreateDIBSection(hdc, bmi, DIB_RGB_COLORS, bits, None, 0).ok()? };
        (!handle.0.is_null()).then_some(Self { handle })
    }
}

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        let _ = unsafe { DeleteObject(self.handle) };
    }
}

struct ObjectSelection {
    hdc: HDC,
    old_object: HGDIOBJ,
}

impl ObjectSelection {
    fn select(hdc: HDC, bitmap: HBITMAP) -> Option<Self> {
        let old_object = unsafe { SelectObject(hdc, bitmap) };
        (!old_object.0.is_null()).then_some(Self { hdc, old_object })
    }
}

impl Drop for ObjectSelection {
    fn drop(&mut self) {
        let _ = unsafe { SelectObject(self.hdc, self.old_object) };
    }
}

fn update_window_bitmap(hwnd: HWND, state: &OsdWindowState, alpha: u8) {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let scale_factor = dpi as f32 / 96.0;

    let physical_width = (BASE_SIZE * scale_factor).round() as u32;
    let physical_height = (BASE_SIZE * scale_factor).round() as u32;

    if physical_width == 0 || physical_height == 0 {
        return;
    }

    let icon_bytes = state.params.icon.map(|token| token.as_bytes());

    let Some(pixel_data) = render_svg_to_bytes(
        physical_width,
        physical_height,
        state.params.total_steps,
        state.params.active_steps,
        &state.params.label,
        icon_bytes,
    ) else {
        return;
    };

    let Some(screen_dc) = DesktopDc::acquire() else {
        return;
    };
    let Some(mem_dc) = CompatibleDc::create(screen_dc.hdc) else {
        return;
    };

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: physical_width as i32,
            biHeight: -(physical_height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let Some(bitmap) = OwnedBitmap::create_dib_section(mem_dc.hdc, &bmi, &mut bits) else {
        return;
    };
    if bits.is_null() {
        return;
    }
    let Some(_selection) = ObjectSelection::select(mem_dc.hdc, bitmap.handle) else {
        return;
    };

    unsafe {
        std::ptr::copy_nonoverlapping(pixel_data.as_ptr(), bits as *mut u8, pixel_data.len());
    }

    let mut window_rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window_rect) }.is_err() {
        return;
    }
    let ppt_dst = POINT {
        x: window_rect.left,
        y: window_rect.top,
    };
    let size = SIZE {
        cx: physical_width as i32,
        cy: physical_height as i32,
    };
    let ppt_src = POINT { x: 0, y: 0 };

    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: alpha,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    let _ = unsafe {
        UpdateLayeredWindow(
            hwnd,
            screen_dc.hdc,
            Some(&ppt_dst),
            Some(&size),
            mem_dc.hdc,
            Some(&ppt_src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        )
    };
}

fn center_window(hwnd: HWND) {
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        let scale = dpi as f32 / 96.0;
        let size = (BASE_SIZE * scale) as i32;

        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);

        let x = (screen_width - size) / 2;
        let y = (screen_height - size) * 5 / 6;

        if let Err(error) = SetWindowPos(hwnd, HWND_TOPMOST, x, y, size, size, SWP_NOACTIVATE) {
            warn!(?error, "Failed to position OSD window");
        }
    }
}

fn internal_show_ui(hwnd: HWND, state: &mut OsdWindowState) {
    unsafe {
        let _ = KillTimer(hwnd, ANIMATION_TIMER);
        state.animation = AnimState::FadeIn;
        state.animation_started_at = Some(Instant::now());
        state.alpha = 0;

        center_window(hwnd);
        update_window_bitmap(hwnd, state, 0);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        if let Err(error) = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        ) {
            warn!(?error, "Failed to show OSD window without activation");
        }
        let _ = SetTimer(hwnd, ANIMATION_TIMER, 10, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn osd_extended_style_preserves_click_through_layered_noactivate_bits() {
        assert_ne!(OSD_EX_STYLE.0 & WS_EX_TRANSPARENT.0, 0);
        assert_ne!(OSD_EX_STYLE.0 & WS_EX_LAYERED.0, 0);
        assert_ne!(OSD_EX_STYLE.0 & WS_EX_NOACTIVATE.0, 0);
    }

    #[test]
    fn stop_clears_osd_running_flag_without_initialized_window() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        OSD_RUNNING.store(true, Ordering::SeqCst);

        OsdController::stop();

        assert!(!OSD_RUNNING.load(Ordering::SeqCst));
    }

    #[test]
    fn join_osd_thread_drains_handle() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *osd_thread() = Some(thread::spawn(|| {}));

        join_osd_thread();

        assert!(osd_thread().is_none());
    }
}
