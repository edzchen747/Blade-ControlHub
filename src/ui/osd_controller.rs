use resvg::tiny_skia;
use std::borrow::Cow;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::UI::HiDpi::*;
use windows::{
    Win32::{
        Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*,
    },
    core::PCWSTR,
};

use crate::ui::icons::OsdIcon;

// Global instance manager allowing static methods to access the internal background thread
static OSD_INSTANCE: OnceLock<OsdController> = OnceLock::new();

// Global layout states required by the Win32 context callback loop
static OSD_LABEL: Mutex<String> = Mutex::new(String::new());
static mut OSD_TOTAL_STEPS: usize = 0;
static mut OSD_ACTIVE_STEPS: usize = 0;
static OSD_ICON: Mutex<Option<OsdIcon>> = Mutex::new(None);

const WM_TRIGGER_OSD: u32 = WM_USER + 1;
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

pub struct OsdParams {
    pub label: String,
    pub total_steps: usize,
    pub active_steps: usize,
    pub icon: Option<OsdIcon>,
}

static SVG_OPTIONS: OnceLock<resvg::usvg::Options<'static>> = OnceLock::new();
static mut STATE: AnimState = AnimState::Idle;
static mut STATE_START_TIME: Option<Instant> = None;
static mut GLOBAL_ALPHA: u8 = 0;

// --- ENCAPSULATED OSD CONTROLLER CLASS ---

pub struct OsdController {
    hwnd: HWND,
}

// SAFETY: PostMessageW is completely thread-safe in Windows, so we can
// safely tell Rust that our controller can be sent and shared across threads.
unsafe impl Send for OsdController {}
unsafe impl Sync for OsdController {}

impl OsdController {
    /// PUBLIC STATIC METHOD: Call this from ANY thread without initializing an instance.
    /// Example: `OsdController::show(params);`
    pub fn show(params: OsdParams) {
        // Automatically fetch or initialize the internal static instance safely
        let controller = OSD_INSTANCE.get_or_init(|| Self::init_internal());

        // Post the cross-thread signal to our dedicated UI window loop
        unsafe {
            let boxed_params = Box::new(params);
            let lparam = LPARAM(Box::into_raw(boxed_params) as isize);

            if PostMessageW(controller.hwnd, WM_TRIGGER_OSD, WPARAM(0), lparam).is_err() {
                // Prevent memory leaks if the target window receiver pipeline goes offline
                let _ = Box::from_raw(lparam.0 as *mut OsdParams);
            }
        }
    }

    /// Internal private initializer that handles background thread setup and affinity constraints
    fn init_internal() -> Self {
        struct SendableHwnd(HWND);
        unsafe impl Send for SendableHwnd {}

        let (tx, rx) = std::sync::mpsc::channel::<SendableHwnd>();

        // Win32 UI elements must live on a dedicated thread with an ongoing message pump loop
        thread::spawn(move || unsafe {
            let _ = get_svg_options();
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

            let class_name = to_wstring("RustOSD_SVG");
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();

            let wc = WNDCLASSW {
                lpfnWndProc: Some(Self::window_proc),
                hInstance: instance,
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };

            RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_LAYERED
                    | WS_EX_TRANSPARENT,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(to_wstring("Rust OSD").as_ptr()),
                WS_POPUP,
                0,
                0,
                BASE_SIZE as i32,
                BASE_SIZE as i32,
                None,
                None,
                instance,
                None,
            )
            .expect("CreateWindowExW failed");

            center_window(hwnd);
            ShowWindow(hwnd, SW_HIDE);

            // Wrap the window reference and send it back to the initial thread
            let _ = tx.send(SendableHwnd(hwnd));

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });

        // Unwrap our sendable wrapper back into a standard HWND
        let SendableHwnd(hwnd) = rx
            .recv()
            .expect("Failed to bind UI worker runtime engine context");
        OsdController { hwnd }
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
                    update_window_bitmap(hwnd, GLOBAL_ALPHA);
                    EndPaint(hwnd, &ps);
                    return LRESULT(0);
                }

                WM_TIMER => {
                    if wparam.0 as usize == ANIMATION_TIMER {
                        if let Some(start_time) = STATE_START_TIME {
                            let elapsed = start_time.elapsed();
                            match STATE {
                                AnimState::FadeIn => {
                                    if elapsed >= FADE_IN_DURATION {
                                        STATE = AnimState::Hold;
                                        STATE_START_TIME = Some(Instant::now());
                                        GLOBAL_ALPHA = TARGET_ALPHA;
                                    } else {
                                        let progress =
                                            elapsed.as_secs_f32() / FADE_IN_DURATION.as_secs_f32();
                                        GLOBAL_ALPHA = (progress * TARGET_ALPHA as f32) as u8;
                                    }
                                    update_window_bitmap(hwnd, GLOBAL_ALPHA);
                                }
                                AnimState::Hold => {
                                    if elapsed >= HOLD_DURATION {
                                        STATE = AnimState::FadeOut;
                                        STATE_START_TIME = Some(Instant::now());
                                    }
                                }
                                AnimState::FadeOut => {
                                    if elapsed >= FADE_OUT_DURATION {
                                        STATE = AnimState::Idle;
                                        STATE_START_TIME = None;
                                        KillTimer(hwnd, ANIMATION_TIMER);
                                        ShowWindow(hwnd, SW_HIDE);
                                    } else {
                                        let progress =
                                            elapsed.as_secs_f32() / FADE_OUT_DURATION.as_secs_f32();
                                        GLOBAL_ALPHA =
                                            ((1.0 - progress) * TARGET_ALPHA as f32) as u8;
                                        update_window_bitmap(hwnd, GLOBAL_ALPHA);
                                    }
                                }
                                AnimState::Idle => {}
                            }
                        }
                    }
                    return LRESULT(0);
                }

                WM_TRIGGER_OSD => {
                    if lparam.0 != 0 {
                        let params = Box::from_raw(lparam.0 as *mut OsdParams);

                        *OSD_LABEL.lock().unwrap() = params.label;
                        *OSD_ICON.lock().unwrap() = params.icon;

                        OSD_TOTAL_STEPS = params.total_steps;
                        OSD_ACTIVE_STEPS = params.active_steps;

                        if STATE == AnimState::Hold || STATE == AnimState::FadeIn {
                            STATE = AnimState::Hold;
                            STATE_START_TIME = Some(Instant::now());
                            update_window_bitmap(hwnd, GLOBAL_ALPHA);
                        } else {
                            internal_show_ui(hwnd);
                        }
                    }
                    return LRESULT(0);
                }

                WM_SYSCOMMAND => {
                    if (wparam.0 & 0xfff0) == SC_CLOSE as usize {
                        return LRESULT(0);
                    }
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    return LRESULT(0);
                }
                _ => {}
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

// --- Shared Utility Implementations ---

fn to_wstring(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
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
) -> Vec<u8> {
    let opt = get_svg_options();

    let bg_data = include_bytes!("../../assets/frame.svg");
    let bg_tree =
        resvg::usvg::Tree::from_data(bg_data, opt).expect("Failed to parse background SVG");

    let text_svg = generate_text_layer_svg(label, icon_bytes.is_none());
    let text_tree = resvg::usvg::Tree::from_data(text_svg.as_bytes(), opt)
        .expect("Failed to parse text SVG layer");

    let progress_svg = generate_progress_svg(total_steps, active_steps);
    let progress_tree = resvg::usvg::Tree::from_data(progress_svg.as_bytes(), opt)
        .expect("Failed to parse progress SVG layer");

    let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();

    let view_scale_x = width as f32 / bg_tree.size().width();
    let view_scale_y = height as f32 / bg_tree.size().height();
    let base_transform = tiny_skia::Transform::from_scale(view_scale_x, view_scale_y);

    resvg::render(&bg_tree, base_transform, &mut pixmap.as_mut());

    if let Some(bytes) = icon_bytes {
        if let Ok(icon_tree) = resvg::usvg::Tree::from_data(&bytes, opt) {
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

    bgra_pixels
}

unsafe fn update_window_bitmap(hwnd: HWND, alpha: u8) {
    let dpi = GetDpiForWindow(hwnd);
    let scale_factor = dpi as f32 / 96.0;

    let physical_width = (BASE_SIZE * scale_factor).round() as u32;
    let physical_height = (BASE_SIZE * scale_factor).round() as u32;

    if physical_width == 0 || physical_height == 0 {
        return;
    }

    let label_guard = OSD_LABEL.lock().unwrap();
    let icon_guard = OSD_ICON.lock().unwrap();
    let icon_bytes = icon_guard.map(|token| token.as_bytes());

    let pixel_data = render_svg_to_bytes(
        physical_width,
        physical_height,
        OSD_TOTAL_STEPS,
        OSD_ACTIVE_STEPS,
        &label_guard, // This is now a safe reference protected by the Mutex!
        icon_bytes,
    );

    let screen_hdc = GetDC(None);
    let mem_hdc = CreateCompatibleDC(screen_hdc);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: physical_width as i32,
            biHeight: -(physical_height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let h_bitmap = CreateDIBSection(mem_hdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
    let old_bitmap = SelectObject(mem_hdc, h_bitmap);

    std::ptr::copy_nonoverlapping(pixel_data.as_ptr(), bits as *mut u8, pixel_data.len());

    let mut window_rect = RECT::default();
    GetWindowRect(hwnd, &mut window_rect).unwrap();
    let mut ppt_dst = POINT {
        x: window_rect.left,
        y: window_rect.top,
    };
    let mut size = SIZE {
        cx: physical_width as i32,
        cy: physical_height as i32,
    };
    let mut ppt_src = POINT { x: 0, y: 0 };

    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: alpha,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    UpdateLayeredWindow(
        hwnd,
        screen_hdc,
        Some(&mut ppt_dst),
        Some(&mut size),
        mem_hdc,
        Some(&mut ppt_src),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );

    SelectObject(mem_hdc, old_bitmap);
    DeleteObject(h_bitmap);
    DeleteDC(mem_hdc);
    ReleaseDC(None, screen_hdc);
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

        SetWindowPos(hwnd, HWND_TOPMOST, x, y, size, size, SWP_NOACTIVATE).unwrap();
    }
}

fn internal_show_ui(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(hwnd, ANIMATION_TIMER);
        STATE = AnimState::FadeIn;
        STATE_START_TIME = Some(Instant::now());
        GLOBAL_ALPHA = 0;

        center_window(hwnd);
        update_window_bitmap(hwnd, 0);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
        .unwrap();
        let _ = SetTimer(hwnd, ANIMATION_TIMER, 10, None);
    }
}
