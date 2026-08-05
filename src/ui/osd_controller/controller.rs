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

const FADE_IN_DURATION: Duration = Duration::from_millis(150);
const HOLD_DURATION: Duration = Duration::from_millis(1500);
const FADE_OUT_DURATION: Duration = Duration::from_millis(500);
const SWAP_FADE_OUT_DURATION: Duration = Duration::from_millis(180);
/// Delay after the swap fade-out before the new front card fades in.
const SWAP_IN_DELAY: Duration = Duration::from_millis(30);
/// The front card's stage-1 slide to the halfway line runs on the swap's own
/// timeline, landing on the midpoint exactly when the depths swap — it must
/// never overlap stage 2's slide-in.
const DEPTH_SWAP_SLIDE_DURATION: Duration = Duration::from_millis(
    SWAP_FADE_OUT_DURATION.as_millis() as u64 + SWAP_IN_DELAY.as_millis() as u64,
);
/// Envelope level the sliding front card dims to during a swap (swap-only).
const SWAP_BACK_ALPHA: u8 = 140;
/// Distance (96-dpi px) the new front card rises while fading in during the
/// swap's second stage.
const SWAP_IN_SLIDE_PX: f32 = 24.0;

const BASE_SIZE: f32 = 200.0;
const ICON_TARGET_WIDTH: f32 = 60.0;
const ICON_TARGET_HEIGHT: f32 = 60.0;

// Card-stack depth tuning: every background level shrinks, darkens and rises.
const DEPTH_SCALE: f32 = 0.85;
const DEPTH_ALPHA: f32 = 0.3;
const DEPTH_OFFSET: f32 = 22.0;

// Every OSD animation (depth slides, swap slide-up, fade envelopes) is a
// duration-based tween driven by a smooth ease-in-out curve: slow start,
// fastest in the middle, slow landing.
const DEPTH_EASE_DURATION: Duration = Duration::from_millis(500);

// Rasterization size grid (physical px). Size-animation ticks only re-render
// when the card crosses a grid line, so easing stays cheap; the window size
// always matches the rendered size.
const RENDER_GRID: u32 = 4;

#[derive(Clone, Copy, PartialEq)]
enum AnimState {
    FadeIn,
    Hold,
    FadeOut,
    /// Stage timing for a pair swap: during `SWAP_FADE_OUT_DURATION` the rear
    /// card fades out (`swap_fade`) while the front card slides to the
    /// midpoint; after an additional `SWAP_IN_DELAY` beat the depths swap
    /// (`swap_to`) and the rear card fades back in at the front while the
    /// front card continues sliding to the rear.
    SwapOut,
    Idle,
}

/// A single stacked OSD card. `depth`/`target_depth` animate towards the
/// card's resting place in the stack: 0 is the front card, growing values
/// recede (smaller, higher, more transparent). `envelope` tracks the
/// fade-in/hold/fade-out lifecycle; `alpha` is the envelope blended with the
/// depth fade and is recomputed from scratch every tick.
struct OsdCard {
    hwnd: HWND,
    kind: Option<u8>,
    params: OsdParams,
    animation: AnimState,
    animation_started_at: Option<Instant>,
    envelope: u8,
    alpha: u8,
    depth: f32,
    target_depth: f32,
    /// Eased depth transition state: `depth` tweens from `depth_from` toward
    /// `target_depth` over `depth_tween_duration`. Re-targeting mid-flight
    /// continues from the current animated value.
    depth_from: f32,
    depth_started_at: Option<Instant>,
    /// Duration of the current depth tween. Defaults to `DEPTH_EASE_DURATION`;
    /// the pair swap's stage-1 slide uses `DEPTH_SWAP_SLIDE_DURATION`.
    depth_tween_duration: Duration,
    /// Envelope value captured when a fade-out phase starts, so quick and slow
    /// fades begin exactly where the card is instead of jumping.
    fade_out_from: u8,
    /// (landing, final) depth pair applied when the current `SwapOut`
    /// completes: the card appears at `landing` while invisible, then eases
    /// to `final` during the fade-in. Pair swaps move at most one card per
    /// phase so only one card re-renders at a time.
    swap_to: Option<(f32, f32)>,
    /// Whether the card fades out during `SwapOut`. The rear card of a pair
    /// swap fades (then fades back in at the front); the front card stays
    /// visible, dims to `SWAP_BACK_ALPHA` while sliding back, and only slides.
    swap_fade: bool,
    /// Vertical rise (96-dpi px) of the new front card while it fades in at
    /// the swap's second stage: tweens from `SWAP_IN_SLIDE_PX` to 0 over
    /// `DEPTH_EASE_DURATION`.
    slide_up: f32,
    slide_up_target: f32,
    slide_up_from: f32,
    slide_up_started_at: Option<Instant>,
    dirty: bool,
    last_alpha: u8,
    last_depth: f32,
    render: Option<CardRendering>,
    layers: Option<SvgLayers>,
}

impl OsdCard {
    /// The stack depth this card is settling at, resolving any in-flight
    /// swap. Swap math must use settle depths (not the animated `depth`) so
    /// rapid consecutive triggers keep whole levels instead of landing on
    /// fractional depths that look like one unstacked card.
    fn settle_depth(&self) -> f32 {
        self.swap_to.map_or(self.target_depth, |(_, settle)| settle)
    }
}

/// Shared state for all OSD cards, owned by the hidden controller window.
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
    /// Identity of the control this OSD represents. Updates with the same kind
    /// replace the front card in place; different kinds stack behind it.
    fn kind(&self) -> Option<u8> {
        self.icon.map(|icon| icon.kind_key())
    }
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

    /// Pure procedural Win32 engine callback handler. The hidden controller
    /// window owns the stack state; each visible card is a separate layered
    /// window of the same class with no user data of its own.
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
                    // Only the controller window carries the stack in user data.
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
        assert_eq!(mic_on.kind(), mic_off.kind());

        let trackpad_on = params(Some(OsdIcon::Trackpad(true)), "");
        let trackpad_off = params(Some(OsdIcon::Trackpad(false)), "");
        assert_eq!(trackpad_on.kind(), trackpad_off.kind());
    }

    #[test]
    fn different_controls_have_distinct_identities() {
        let brightness = params(Some(OsdIcon::Brightness), "");
        let keyboard = params(Some(OsdIcon::KeyboardBrightness), "");
        assert_ne!(brightness.kind(), keyboard.kind());
    }

    #[test]
    fn icon_less_osds_share_one_slot() {
        let perf_mode = params(None, "Balanced");
        let rgb_effect = params(None, "Wave");
        assert_eq!(perf_mode.kind(), rgb_effect.kind());
        assert_eq!(perf_mode.kind(), None);
    }
}
