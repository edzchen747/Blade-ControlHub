use resvg::tiny_skia;
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing::warn;
use windows::Win32::UI::HiDpi::*;
use windows::{
    Win32::{
        Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*,
    },
    core::PCWSTR,
};

use crate::ui::icons::OsdIcon;

// Global instance manager allowing static methods to access the internal background thread
static OSD_INSTANCE: OnceLock<Option<OsdController>> = OnceLock::new();
static OSD_RUNNING: AtomicBool = AtomicBool::new(false);
static OSD_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

const WM_TRIGGER_OSD: u32 = WM_USER + 1;
const WM_STOP_OSD: u32 = WM_USER + 2;
const OSD_EX_STYLE: WINDOW_EX_STYLE = WINDOW_EX_STYLE(
    WS_EX_TOPMOST.0
        | WS_EX_TOOLWINDOW.0
        | WS_EX_NOACTIVATE.0
        | WS_EX_LAYERED.0
        | WS_EX_TRANSPARENT.0,
);
const DESIGN_SIZE: f32 = 150.0;
const ANIMATION_TIMER: usize = 1;
const TARGET_ALPHA: u8 = 230;

const FADE_IN_DURATION: Duration = Duration::from_millis(100);
const HOLD_DURATION: Duration = Duration::from_millis(1500);
const FADE_OUT_DURATION: Duration = Duration::from_millis(300);

const BASE_SIZE: f32 = 200.0;
const ICON_TARGET_WIDTH: f32 = 60.0;
const ICON_TARGET_HEIGHT: f32 = 60.0;

#[derive(Clone, Copy, PartialEq)]
enum AnimState {
    FadeIn,
    Hold,
    FadeOut,
    Idle,
}

struct OsdWindowState {
    params: OsdParams,
    animation: AnimState,
    animation_started_at: Option<Instant>,
    alpha: u8,
}

impl Default for OsdWindowState {
    fn default() -> Self {
        Self {
            params: OsdParams {
                label: String::new(),
                total_steps: 0,
                active_steps: 0,
                icon: None,
            },
            animation: AnimState::Idle,
            animation_started_at: None,
            alpha: 0,
        }
    }
}

pub struct OsdParams {
    pub label: String,
    pub total_steps: usize,
    pub active_steps: usize,
    pub icon: Option<OsdIcon>,
}

static SVG_OPTIONS: OnceLock<resvg::usvg::Options<'static>> = OnceLock::new();

// --- ENCAPSULATED OSD CONTROLLER CLASS ---

pub struct OsdController {
    hwnd: HWND,
}

struct SendableHwnd(HWND);

// SAFETY: HWND values are opaque OS handles. The OSD controller only posts
// messages to the window from other threads; all window state is owned and
// mutated by the dedicated OSD window thread.
unsafe impl Send for SendableHwnd {}

// SAFETY: PostMessageW is completely thread-safe in Windows, so we can
// safely tell Rust that our controller can be sent and shared across threads.
unsafe impl Send for OsdController {}
unsafe impl Sync for OsdController {}

impl OsdController {
    /// PUBLIC STATIC METHOD: Call this from ANY thread without initializing an instance.
    /// Example: `OsdController::show(params);`
    pub fn show(params: OsdParams) {
        // Automatically fetch or initialize the internal static instance safely
        let controller = OSD_INSTANCE.get_or_init(Self::init_internal);
        let Some(controller) = controller.as_ref() else {
            warn!("OSD window is unavailable; dropping OSD update");
            return;
        };
        if !OSD_RUNNING.load(Ordering::SeqCst) {
            warn!("OSD window is stopped; dropping OSD update");
            return;
        }

        // Post the cross-thread signal to our dedicated UI window loop
        post_osd_update(controller.hwnd, params);
    }

    pub fn stop() {
        OSD_RUNNING.store(false, Ordering::SeqCst);

        if let Some(Some(controller)) = OSD_INSTANCE.get() {
            post_osd_stop(controller.hwnd);
        }
        join_osd_thread();
    }

    /// Internal private initializer that handles background thread setup and affinity constraints
    fn init_internal() -> Option<Self> {
        let (tx, rx) = std::sync::mpsc::channel::<Option<SendableHwnd>>();

        let handle = match thread::Builder::new()
            .name("blade-osd-window".to_string())
            .spawn(move || run_osd_window_thread(tx))
        {
            Ok(handle) => handle,
            Err(error) => {
                warn!(%error, "Failed to start OSD window thread");
                return None;
            }
        };

        // Unwrap our sendable wrapper back into a standard HWND
        let Some(SendableHwnd(hwnd)) = rx.recv_timeout(Duration::from_secs(2)).ok().flatten()
        else {
            warn!("Timed out while creating OSD window thread");
            if handle.is_finished() {
                let _ = handle.join();
            }
            return None;
        };
        *osd_thread() = Some(handle);
        Some(OsdController { hwnd })
    }

    /// Pure procedural Win32 engine callback handler
    extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe {
            match msg {
                WM_NCHITTEST => return LRESULT(HTTRANSPARENT as isize),
                WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),

                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    BeginPaint(hwnd, &mut ps);
                    with_osd_state(hwnd, |state| {
                        update_window_bitmap(hwnd, state, state.alpha);
                    });
                    let _ = EndPaint(hwnd, &ps);
                    return LRESULT(0);
                }

                WM_TIMER => {
                    if wparam.0 == ANIMATION_TIMER {
                        with_osd_state(hwnd, |state| {
                            let Some(start_time) = state.animation_started_at else {
                                return;
                            };

                            let elapsed = start_time.elapsed();
                            match state.animation {
                                AnimState::FadeIn => {
                                    if elapsed >= FADE_IN_DURATION {
                                        state.animation = AnimState::Hold;
                                        state.animation_started_at = Some(Instant::now());
                                        state.alpha = TARGET_ALPHA;
                                    } else {
                                        let progress =
                                            elapsed.as_secs_f32() / FADE_IN_DURATION.as_secs_f32();
                                        state.alpha = (progress * TARGET_ALPHA as f32) as u8;
                                    }
                                    update_window_bitmap(hwnd, state, state.alpha);
                                }
                                AnimState::Hold => {
                                    if elapsed >= HOLD_DURATION {
                                        state.animation = AnimState::FadeOut;
                                        state.animation_started_at = Some(Instant::now());
                                    }
                                }
                                AnimState::FadeOut => {
                                    if elapsed >= FADE_OUT_DURATION {
                                        state.animation = AnimState::Idle;
                                        state.animation_started_at = None;
                                        let _ = KillTimer(hwnd, ANIMATION_TIMER);
                                        let _ = ShowWindow(hwnd, SW_HIDE);
                                    } else {
                                        let progress =
                                            elapsed.as_secs_f32() / FADE_OUT_DURATION.as_secs_f32();
                                        state.alpha =
                                            ((1.0 - progress) * TARGET_ALPHA as f32) as u8;
                                        update_window_bitmap(hwnd, state, state.alpha);
                                    }
                                }
                                AnimState::Idle => {}
                            }
                        });
                    }
                    return LRESULT(0);
                }

                WM_TRIGGER_OSD => {
                    if lparam.0 != 0 {
                        let params = Box::from_raw(lparam.0 as *mut OsdParams);

                        with_osd_state(hwnd, |state| {
                            state.params = *params;

                            if state.animation == AnimState::Hold
                                || state.animation == AnimState::FadeIn
                            {
                                state.animation = AnimState::Hold;
                                state.animation_started_at = Some(Instant::now());
                                update_window_bitmap(hwnd, state, state.alpha);
                            } else {
                                internal_show_ui(hwnd, state);
                            }
                        });
                    }
                    return LRESULT(0);
                }

                WM_STOP_OSD => {
                    let _ = KillTimer(hwnd, ANIMATION_TIMER);
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }

                WM_SYSCOMMAND => {
                    if (wparam.0 & 0xfff0) == SC_CLOSE as usize {
                        return LRESULT(0);
                    }
                }
                WM_DESTROY => {
                    drop_osd_state(hwnd);
                    OSD_RUNNING.store(false, Ordering::SeqCst);
                    PostQuitMessage(0);
                    return LRESULT(0);
                }
                _ => {}
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

fn osd_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    OSD_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_osd_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = osd_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current OSD window thread during shutdown");
        *osd_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("OSD window thread panicked during shutdown");
    }
}

// --- Shared Utility Implementations ---

fn post_osd_update(hwnd: HWND, params: OsdParams) {
    unsafe {
        let boxed_params = Box::new(params);
        let lparam = LPARAM(Box::into_raw(boxed_params) as isize);

        if PostMessageW(hwnd, WM_TRIGGER_OSD, WPARAM(0), lparam).is_err() {
            // Prevent memory leaks if the target window receiver pipeline goes offline.
            let _ = Box::from_raw(lparam.0 as *mut OsdParams);
        }
    }
}

fn post_osd_stop(hwnd: HWND) {
    let _ = unsafe { PostMessageW(hwnd, WM_STOP_OSD, WPARAM(0), LPARAM(0)) };
}

fn run_osd_window_thread(tx: Sender<Option<SendableHwnd>>) {
    let _ = get_svg_options();

    let hwnd = match create_osd_window() {
        Some(hwnd) => hwnd,
        None => {
            let _ = tx.send(None);
            return;
        }
    };

    let state = Box::new(OsdWindowState::default());
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
    }

    center_window(hwnd);
    let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };

    OSD_RUNNING.store(true, Ordering::SeqCst);
    let _ = tx.send(Some(SendableHwnd(hwnd)));
    run_osd_message_loop();
    OSD_RUNNING.store(false, Ordering::SeqCst);
}

fn create_osd_window() -> Option<HWND> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let class_name = to_wstring("RustOSD_SVG");
        let title = to_wstring("Rust OSD");
        let instance: HINSTANCE = match GetModuleHandleW(None) {
            Ok(handle) => handle.into(),
            Err(error) => {
                warn!(?error, "Failed to acquire module handle for OSD window");
                return None;
            }
        };

        let wc = WNDCLASSW {
            lpfnWndProc: Some(OsdController::window_proc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };

        RegisterClassW(&wc);

        match CreateWindowExW(
            OSD_EX_STYLE,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            0,
            0,
            BASE_SIZE as i32,
            BASE_SIZE as i32,
            None,
            None,
            instance,
            None,
        ) {
            Ok(hwnd) => Some(hwnd),
            Err(error) => {
                warn!(?error, "Failed to create OSD layered window");
                None
            }
        }
    }
}

fn run_osd_message_loop() {
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn to_wstring(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn with_osd_state<R>(hwnd: HWND, action: impl FnOnce(&mut OsdWindowState) -> R) -> Option<R> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OsdWindowState };
    if ptr.is_null() {
        return None;
    }

    // SAFETY: The pointer is installed from a Box in `run_osd_window_thread`
    // and cleared in `drop_osd_state`. OSD window state is only accessed from
    // this window procedure on the dedicated OSD thread.
    Some(action(unsafe { &mut *ptr }))
}

fn drop_osd_state(hwnd: HWND) {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OsdWindowState };
    if !ptr.is_null() {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(ptr));
        }
    }
}

fn get_svg_options() -> &'static resvg::usvg::Options<'static> {
    SVG_OPTIONS.get_or_init(|| {
        let mut opt = resvg::usvg::Options::default();
        *opt.fontdb_mut() = generate_font_db();
        opt.font_family = "Roboto".to_string();
        opt
    })
}

fn generate_font_db() -> resvg::usvg::fontdb::Database {
    let mut font_db = resvg::usvg::fontdb::Database::new();
    let font_bytes = include_bytes!("../../assets/Roboto.ttf");
    font_db.load_font_data(font_bytes.to_vec());
    font_db
}

fn generate_text_layer_svg(label: &str, no_icon: bool) -> String {
    let font_family_target = "Roboto";
    let y_pos = if no_icon { 85 } else { 115 };
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%">
            <text x="{}" y="{}" font-family="{}, sans-serif" font-size="20" font-weight="bold" fill="white" text-anchor="middle">{}</text>
        </svg>"#,
        DESIGN_SIZE,
        DESIGN_SIZE,
        (DESIGN_SIZE / 2.0),
        y_pos,
        font_family_target,
        label
    )
}

fn generate_progress_svg(total_steps: usize, active_steps: usize) -> String {
    if total_steps == 0 {
        return format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%"></svg>"#,
            DESIGN_SIZE, DESIGN_SIZE
        );
    }

    let padding_x = 15.0;
    let bar_y: f64 = 122.0;
    let block_h = 6.0;
    let available_width = DESIGN_SIZE - (padding_x * 2.0);

    let gap = 1.5;
    let total_gaps_width = gap * (total_steps - 1) as f32;
    let block_w = (available_width - total_gaps_width) / total_steps as f32;

    if block_w <= 0.0 {
        return format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%"></svg>"#,
            DESIGN_SIZE, DESIGN_SIZE
        );
    }

    let mut svg_string = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%">"#,
        DESIGN_SIZE, DESIGN_SIZE
    );

    for i in 0..total_steps {
        let x_pos = padding_x + i as f32 * (block_w + gap);
        let fill_color = if i < active_steps {
            "#F1C40F"
        } else {
            "#FFFFFF"
        };
        let fill_opacity = if i < active_steps { "1.0" } else { "0.2" };

        svg_string.push_str(&format!(
            r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="1" fill="{}" fill-opacity="{}" />"#,
            x_pos, bar_y, block_w, block_h, fill_color, fill_opacity
        ));
    }

    svg_string.push_str("</svg>");
    svg_string
}

fn render_svg_to_bytes(
    width: u32,
    height: u32,
    total_steps: usize,
    active_steps: usize,
    label: &str,
    icon_bytes: Option<Cow<'static, [u8]>>,
) -> Option<Vec<u8>> {
    let opt = get_svg_options();

    let bg_data = include_bytes!("../../assets/frame.svg");
    let bg_tree = match resvg::usvg::Tree::from_data(bg_data, opt) {
        Ok(tree) => tree,
        Err(error) => {
            warn!(?error, "Failed to parse OSD frame SVG");
            return None;
        }
    };

    let text_svg = generate_text_layer_svg(label, icon_bytes.is_none());
    let text_tree = match resvg::usvg::Tree::from_data(text_svg.as_bytes(), opt) {
        Ok(tree) => tree,
        Err(error) => {
            warn!(?error, "Failed to parse generated OSD text SVG");
            return None;
        }
    };

    let progress_svg = generate_progress_svg(total_steps, active_steps);
    let progress_tree = match resvg::usvg::Tree::from_data(progress_svg.as_bytes(), opt) {
        Ok(tree) => tree,
        Err(error) => {
            warn!(?error, "Failed to parse generated OSD progress SVG");
            return None;
        }
    };

    let Some(mut pixmap) = tiny_skia::Pixmap::new(width, height) else {
        warn!(width, height, "Failed to allocate OSD pixmap");
        return None;
    };

    let view_scale_x = width as f32 / bg_tree.size().width();
    let view_scale_y = height as f32 / bg_tree.size().height();
    let base_transform = tiny_skia::Transform::from_scale(view_scale_x, view_scale_y);

    resvg::render(&bg_tree, base_transform, &mut pixmap.as_mut());

    if let Some(icon_tree) =
        icon_bytes.and_then(|bytes| resvg::usvg::Tree::from_data(&bytes, opt).ok())
    {
        let bg_width_coords = bg_tree.size().width();
        let icon_scale_x = ICON_TARGET_WIDTH / icon_tree.size().width();
        let icon_scale_y = ICON_TARGET_HEIGHT / icon_tree.size().height();
        let icon_pos_x = (bg_width_coords - ICON_TARGET_WIDTH) / 2.0;
        let icon_pos_y = 32.0;

        let icon_transform = tiny_skia::Transform::from_scale(icon_scale_x, icon_scale_y)
            .post_translate(icon_pos_x, icon_pos_y)
            .post_scale(view_scale_x, view_scale_y);

        resvg::render(&icon_tree, icon_transform, &mut pixmap.as_mut());
    }

    resvg::render(&text_tree, base_transform, &mut pixmap.as_mut());
    resvg::render(&progress_tree, base_transform, &mut pixmap.as_mut());

    let mut bgra_pixels = pixmap.data().to_vec();
    for chunk in bgra_pixels.chunks_exact_mut(4) {
        let r = chunk[0];
        let b = chunk[2];
        chunk[0] = b;
        chunk[2] = r;
    }

    Some(bgra_pixels)
}

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
