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

const FADE_IN_DURATION: Duration = Duration::from_millis(150);
const HOLD_DURATION: Duration = Duration::from_millis(1500);
const FADE_OUT_DURATION: Duration = Duration::from_millis(500);
const SWAP_FADE_OUT_DURATION: Duration = Duration::from_millis(180);
const SWAP_SHOW_NEW_DELAY: Duration = Duration::from_millis(30);
const FRONT_CARD_MIDPOINT_DURATION: Duration = Duration::from_millis(
    SWAP_FADE_OUT_DURATION.as_millis() as u64 + SWAP_SHOW_NEW_DELAY.as_millis() as u64,
);
const RECEDING_CARD_ALPHA: u8 = 140;
const PROMOTED_CARD_Y_OFFSET: f32 = 24.0;

const BASE_SIZE: f32 = 200.0;
const ICON_TARGET_WIDTH: f32 = 60.0;
const ICON_TARGET_HEIGHT: f32 = 60.0;

const STACK_DEPTH_SCALE: f32 = 0.85;
const STACK_DEPTH_ALPHA: f32 = 0.3;
const STACK_DEPTH_Y_OFFSET: f32 = 22.0;

const CARD_TRANSITION_DURATION: Duration = Duration::from_millis(400);

const RENDER_SIZE_GRID_PX: u32 = 4;

#[derive(Clone, Copy, PartialEq)]
enum CardLifecycle {
    FadingIn,
    Holding,
    FadingOut,
    Swapping,
    Expired,
}

struct SwapDestination {
    hidden_stack_depth: f32,
    final_stack_depth: f32,
}

struct OsdCard {
    hwnd: HWND,
    identity: Option<u8>,
    params: OsdParams,
    lifecycle: CardLifecycle,
    lifecycle_started_at: Option<Instant>,
    lifecycle_alpha: u8,
    composited_alpha: u8,
    stack_depth: f32,
    target_stack_depth: f32,
    stack_depth_start: f32,
    stack_depth_started_at: Option<Instant>,
    stack_depth_transition_duration: Duration,
    fade_out_start_alpha: u8,
    swap_destination: Option<SwapDestination>,
    is_promoted_during_swap: bool,
    promotion_y_offset: f32,
    promotion_y_offset_target: f32,
    promotion_y_offset_start: f32,
    promotion_y_offset_started_at: Option<Instant>,
    dirty: bool,
    last_composited_alpha: u8,
    last_stack_depth: f32,
    render: Option<CardRendering>,
    layers: Option<SvgLayers>,
}

impl OsdCard {
    fn final_stack_depth(&self) -> f32 {
        self.swap_destination
            .as_ref()
            .map_or(self.target_stack_depth, |destination| {
                destination.final_stack_depth
            })
    }

    fn fade_in_duration(&self) -> Duration {
        if self.is_promoted_during_swap {
            CARD_TRANSITION_DURATION
        } else {
            FADE_IN_DURATION
        }
    }

    fn fade_in_progress(&self, elapsed: Duration) -> f32 {
        let progress = elapsed.as_secs_f32() / self.fade_in_duration().as_secs_f32();
        if self.is_promoted_during_swap {
            ease_out(progress)
        } else {
            progress
        }
    }
}

#[derive(Default)]
struct OsdStackState {
    cards: Vec<OsdCard>,
}

pub struct OsdParams {
    pub label: String,
    pub total_steps: usize,
    pub active_steps: usize,
    pub icon: Option<OsdIcon>,
}

impl OsdParams {
    fn identity(&self) -> Option<u8> {
        self.icon.map(|icon| icon.kind_key())
    }
}

static SVG_OPTIONS: OnceLock<resvg::usvg::Options<'static>> = OnceLock::new();

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
    pub fn show(params: OsdParams) {
        let controller = OSD_INSTANCE.get_or_init(Self::init_internal);
        let Some(controller) = controller.as_ref() else {
            warn!("OSD window is unavailable; dropping OSD update");
            return;
        };
        if !OSD_RUNNING.load(Ordering::SeqCst) {
            warn!("OSD window is stopped; dropping OSD update");
            return;
        }

        post_osd_update(controller.hwnd, params);
    }

    pub fn stop() {
        OSD_RUNNING.store(false, Ordering::SeqCst);

        if let Some(Some(controller)) = OSD_INSTANCE.get() {
            post_osd_stop(controller.hwnd);
        }
        join_osd_thread();
    }

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
                    with_osd_stack(|stack| {
                        if let Some(card) = stack.cards.iter_mut().find(|card| card.hwnd == hwnd) {
                            card.dirty = true;
                            update_card_window(hwnd, card);
                        }
                    });
                    let _ = EndPaint(hwnd, &ps);
                    return LRESULT(0);
                }

                WM_TIMER => {
                    if wparam.0 == ANIMATION_TIMER {
                        with_osd_stack(|stack| {
                            tick_osd_stack(hwnd, stack);
                        });
                    }
                    return LRESULT(0);
                }

                WM_TRIGGER_OSD => {
                    if lparam.0 != 0 {
                        let mut params = Some(Box::from_raw(lparam.0 as *mut OsdParams));
                        with_osd_stack(|stack| {
                            if let Some(boxed) = params.take() {
                                handle_osd_params(hwnd, stack, *boxed);
                            }
                        });
                    }
                    return LRESULT(0);
                }

                WM_STOP_OSD => {
                    let _ = KillTimer(hwnd, ANIMATION_TIMER);
                    with_osd_stack(|stack| {
                        let card_hwnds: Vec<HWND> =
                            stack.cards.drain(..).map(|card| card.hwnd).collect();
                        for card_hwnd in card_hwnds {
                            let _ = DestroyWindow(card_hwnd);
                        }
                    });
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }

                WM_SYSCOMMAND => {
                    if (wparam.0 & 0xfff0) == SC_CLOSE as usize {
                        return LRESULT(0);
                    }
                }
                WM_DESTROY => {
                    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                    if ptr != 0 {
                        drop_osd_state(hwnd);
                        OSD_RUNNING.store(false, Ordering::SeqCst);
                        PostQuitMessage(0);
                    }
                    return LRESULT(0);
                }
                _ => {}
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use crate::ui::icons::OsdIcon;

    fn params(icon: Option<OsdIcon>, label: &str) -> OsdParams {
        OsdParams {
            label: label.to_string(),
            total_steps: 0,
            active_steps: 0,
            icon,
        }
    }

    #[test]
    fn boolean_state_does_not_change_osd_identity() {
        let mic_on = params(Some(OsdIcon::MicMute(false)), "");
        let mic_off = params(Some(OsdIcon::MicMute(true)), "");
        assert_eq!(mic_on.identity(), mic_off.identity());

        let trackpad_on = params(Some(OsdIcon::Trackpad(true)), "");
        let trackpad_off = params(Some(OsdIcon::Trackpad(false)), "");
        assert_eq!(trackpad_on.identity(), trackpad_off.identity());
    }

    #[test]
    fn different_controls_have_distinct_identities() {
        let brightness = params(Some(OsdIcon::Brightness), "");
        let keyboard = params(Some(OsdIcon::KeyboardBrightness), "");
        assert_ne!(brightness.identity(), keyboard.identity());
    }

    #[test]
    fn icon_less_osds_share_one_slot() {
        let perf_mode = params(None, "Balanced");
        let rgb_effect = params(None, "Wave");
        assert_eq!(perf_mode.identity(), rgb_effect.identity());
        assert_eq!(perf_mode.identity(), None);
    }
}
