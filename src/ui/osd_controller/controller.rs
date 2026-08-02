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

