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

/// Cached DIB section backing one card's layered window. Rebuilt only when the
/// card's physical size changes; alpha and position updates reuse the buffer.
/// Field order matters: `selection` restores the DC's old object first, then
/// the bitmap is deleted, then the DC is released.
///
/// `selection` and `bitmap` are held solely for their RAII drop order.
#[allow(dead_code)]
struct CardRendering {
    selection: ObjectSelection,
    bitmap: OwnedBitmap,
    dc: CompatibleDc,
    bits: *mut std::ffi::c_void,
    size: (u32, u32),
}

impl CardRendering {
    fn create(hdc_screen: HDC, width: u32, height: u32, pixel_data: &[u8]) -> Option<Self> {
        let dc = CompatibleDc::create(hdc_screen)?;

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bitmap = OwnedBitmap::create_dib_section(dc.hdc, &bmi, &mut bits)?;
        let selection = ObjectSelection::select(dc.hdc, bitmap.handle)?;

        unsafe {
            std::ptr::copy_nonoverlapping(pixel_data.as_ptr(), bits as *mut u8, pixel_data.len());
        }

        Some(Self {
            selection,
            bitmap,
            dc,
            bits,
            size: (width, height),
        })
    }

    fn write_pixels(&mut self, pixel_data: &[u8]) {
        if !self.bits.is_null() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    pixel_data.as_ptr(),
                    self.bits as *mut u8,
                    pixel_data.len(),
                );
            }
        }
    }
}

fn update_card_window(hwnd: HWND, card: &mut OsdCard) {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let scale_factor = dpi as f32 / 96.0;
    let depth_scale = DEPTH_SCALE.powf(card.depth);

    let physical_width = ((BASE_SIZE * depth_scale * scale_factor / RENDER_GRID as f32)
        .round()
        .max(1.0) as u32)
        * RENDER_GRID;
    let physical_height = ((BASE_SIZE * depth_scale * scale_factor / RENDER_GRID as f32)
        .round()
        .max(1.0) as u32)
        * RENDER_GRID;

    let size_changed = card
        .render
        .as_ref()
        .is_none_or(|rendering| rendering.size != (physical_width, physical_height));
    let alpha_changed = card.last_alpha != card.alpha;
    let depth_changed = (card.last_depth - card.depth).abs() > 0.0001;
    let theme_changed = card
        .layers
        .as_ref()
        .is_none_or(|layers| layers.theme != crate::ui::theme::runtime_theme_color());

    if !card.dirty && !size_changed && !alpha_changed && !depth_changed && !theme_changed {
        return;
    }

    let Some(screen_dc) = DesktopDc::acquire() else {
        return;
    };

    if card.dirty || size_changed || theme_changed {
        if card.dirty || theme_changed {
            let icon_bytes = card.params.icon.map(|token| token.as_bytes());
            let Some(layers) = SvgLayers::parse(
                &card.params.label,
                card.params.total_steps,
                card.params.active_steps,
                icon_bytes,
            ) else {
                return;
            };
            card.layers = Some(layers);
        }

        let Some(layers) = card.layers.as_ref() else {
            return;
        };
        let Some(pixel_data) = render_layers_to_bytes(layers, physical_width, physical_height)
        else {
            return;
        };

        match card.render.as_mut() {
            Some(rendering) if rendering.size == (physical_width, physical_height) => {
                rendering.write_pixels(&pixel_data);
            }
            _ => {
                let Some(rendering) = CardRendering::create(
                    screen_dc.hdc,
                    physical_width,
                    physical_height,
                    &pixel_data,
                ) else {
                    return;
                };
                card.render = Some(rendering);
            }
        }
    }
    card.dirty = false;

    let Some(rendering) = card.render.as_ref() else {
        return;
    };

    // Depth 0 matches the previous centered placement; deeper cards rise and
    // shrink towards the top of the stack. `slide_up` offsets the new front
    // card below its spot, then eases up to 0 as it fades in.
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let base_px = (BASE_SIZE * scale_factor).round() as i32;
    let base_y = ((screen_height - base_px) * 5) / 6;
    let depth_offset_px = (DEPTH_OFFSET * scale_factor * card.depth).round() as i32;
    let slide_up_px = (card.slide_up * scale_factor).round() as i32;

    let ppt_dst = POINT {
        x: (screen_width - physical_width as i32) / 2,
        y: base_y - depth_offset_px + slide_up_px,
    };
    let size = SIZE {
        cx: physical_width as i32,
        cy: physical_height as i32,
    };
    let ppt_src = POINT { x: 0, y: 0 };

    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: card.alpha,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    let _ = unsafe {
        UpdateLayeredWindow(
            hwnd,
            screen_dc.hdc,
            Some(&ppt_dst),
            Some(&size),
            rendering.dc.hdc,
            Some(&ppt_src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        )
    };

    card.last_alpha = card.alpha;
    card.last_depth = card.depth;
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
