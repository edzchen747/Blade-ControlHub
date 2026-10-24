//! Keeps held keys acting after Windows has stopped repeating them.
//!
//! Windows auto-repeats only the most recently pressed key. Hold brightness up,
//! then press keyboard brightness up as well, and the first stops dead for as
//! long as the second is down — two controls cannot be driven together. This
//! tracks the keys that opted in (`KeyEventAction::repeating`) and, for every
//! one of them that Windows is no longer repeating, fires it again at the
//! system's own repeat rate. The key Windows *is* repeating is left alone, so a
//! single held key behaves exactly as it always did.
//!
//! It also feeds `held_keys`, so the overlay can tell a fresh press from a key
//! that is still being held.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tracing::{error, warn};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETKEYBOARDDELAY, SPI_GETKEYBOARDSPEED, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SystemParametersInfoW,
};

use crate::core::held_keys;
use crate::ui::osd_controller::OsdController;
use crate::win::input::key_map::Repeat;

/// A key whose release was never seen — focus lost mid-press, a locked
/// session — would otherwise repeat for ever. Nothing legitimate is held this
/// long: brightness has long since hit its end stop.
const MAX_HOLD: Duration = Duration::from_secs(30);

static HELD: Mutex<Held> = Mutex::new(Held::new());
static WAKE: Condvar = Condvar::new();
static RUNNING: AtomicBool = AtomicBool::new(false);
static THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Starts firing held keys through `fire`, which returns whether the key is
/// still being handled — false ends its repeating.
pub fn start(fire: fn(u8) -> bool) {
    join_finished_thread();
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    match thread::Builder::new()
        .name("blade-key-repeat".to_string())
        .spawn(move || run(fire))
    {
        Ok(handle) => *thread_slot() = Some(handle),
        Err(error) => {
            RUNNING.store(false, Ordering::SeqCst);
            error!(%error, "Failed to start key repeat thread");
        }
    }
}

pub fn stop() {
    RUNNING.store(false, Ordering::SeqCst);
    // Take the lock before notifying so the thread is either already waiting or
    // has not yet read the flag, never in between and missing the wake.
    drop(held());
    WAKE.notify_all();
    join_thread();

    let cleared = held().clear();
    for key in cleared {
        release_overlay(key.repeat);
    }
}

/// Registers a key press the hook is about to act on. Returns whether it is a
/// fresh press: false means the key was already down, so this is Windows
/// repeating it (or this thread firing it) rather than the user pressing it.
pub fn press(code: u8, repeat: Repeat) -> bool {
    let timing = Timing::system();
    let fresh = held().press(code, repeat, Instant::now(), timing);
    if fresh {
        if let Some(kind) = repeat.osd_kind {
            held_keys::press(kind);
        }
        WAKE.notify_all();
    }
    fresh
}

/// The key went up, or stopped being something the hook handles.
pub fn release(code: u8) {
    let released = held().release(code);
    if let Some(key) = released {
        release_overlay(key.repeat);
        WAKE.notify_all();
    }
}

fn release_overlay(repeat: Repeat) {
    if let Some(kind) = repeat.osd_kind {
        held_keys::release(kind);
        OsdController::held_keys_changed();
    }
}

fn run(fire: fn(u8) -> bool) {
    let mut state = held();
    while RUNNING.load(Ordering::SeqCst) {
        let now = Instant::now();
        let due = state.take_due(now, Timing::system());

        for key in &due.expired {
            warn!(
                code = key.code,
                "Dropping a held key that never reported its release"
            );
            release_overlay(key.repeat);
        }

        if !due.fire.is_empty() {
            // The actions run without the lock: one of them may sleep (a
            // synthesized media key does) and the hook must not wait on it.
            drop(state);
            for code in due.fire {
                if !fire(code) {
                    release(code);
                }
            }
            state = held();
            continue;
        }

        state = match state.next_wake() {
            Some(at) => {
                WAKE.wait_timeout(state, at.saturating_duration_since(now))
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .0
            }
            None => WAKE
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        };
    }
}

fn held() -> MutexGuard<'static, Held> {
    HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn thread_slot() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_thread() {
    let finished = thread_slot().as_ref().is_some_and(JoinHandle::is_finished);
    if finished {
        join_thread();
    }
}

fn join_thread() {
    let Some(handle) = thread_slot().take() else {
        return;
    };
    if handle.thread().id() == thread::current().id() {
        *thread_slot() = Some(handle);
        return;
    }
    if handle.join().is_err() {
        warn!("Key repeat thread panicked during shutdown");
    }
}

/// The user's own keyboard repeat settings, so a key this thread repeats feels
/// the same as one Windows repeats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Timing {
    delay: Duration,
    interval: Duration,
}

impl Timing {
    fn system() -> Self {
        // The control panel's two sliders: delay 0..=3 is 250..=1000 ms, speed
        // 0..=31 is about 2.5..=30 repeats a second.
        let delay = query_u32(SPI_GETKEYBOARDDELAY).unwrap_or(1);
        let speed = query_u32(SPI_GETKEYBOARDSPEED).unwrap_or(31);
        Self::from_settings(delay, speed)
    }

    fn from_settings(delay: u32, speed: u32) -> Self {
        let per_second = 2.5 + f64::from(speed.min(31)) * (27.5 / 31.0);
        Self {
            delay: Duration::from_millis(u64::from(delay.min(3) + 1) * 250),
            interval: Duration::from_secs_f64(1.0 / per_second),
        }
    }
}

fn query_u32(
    action: windows::Win32::UI::WindowsAndMessaging::SYSTEM_PARAMETERS_INFO_ACTION,
) -> Option<u32> {
    let mut value = 0u32;
    unsafe {
        SystemParametersInfoW(
            action,
            0,
            Some(&mut value as *mut u32 as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .ok()
    .map(|()| value)
}

#[derive(Clone, Copy, Debug)]
struct HeldKey {
    code: u8,
    repeat: Repeat,
    pressed_at: Instant,
    /// When this thread should next fire the key, if Windows is not doing it.
    next_fire: Instant,
}

#[derive(Default)]
struct Due {
    fire: Vec<u8>,
    expired: Vec<HeldKey>,
}

struct Held {
    keys: Vec<HeldKey>,
    /// The key Windows is repeating, if any. Only the latest press is.
    last_pressed: Option<u8>,
}

impl Held {
    const fn new() -> Self {
        Self {
            keys: Vec::new(),
            last_pressed: None,
        }
    }

    fn press(&mut self, code: u8, repeat: Repeat, now: Instant, timing: Timing) -> bool {
        if let Some(key) = self.keys.iter_mut().find(|key| key.code == code) {
            // Windows or this thread repeating it: it is still held, and
            // whoever just repeated it has done the next one's work.
            key.next_fire = now + timing.interval;
            return false;
        }

        self.keys.push(HeldKey {
            code,
            repeat,
            pressed_at: now,
            next_fire: now + timing.delay,
        });
        self.last_pressed = Some(code);
        true
    }

    fn release(&mut self, code: u8) -> Option<HeldKey> {
        let index = self.keys.iter().position(|key| key.code == code)?;
        if self.last_pressed == Some(code) {
            self.last_pressed = None;
        }
        Some(self.keys.remove(index))
    }

    fn clear(&mut self) -> Vec<HeldKey> {
        self.last_pressed = None;
        std::mem::take(&mut self.keys)
    }

    /// Everything that needs doing at `now`: held keys to fire again, and keys
    /// that have been down so long they must have been missed on release.
    fn take_due(&mut self, now: Instant, timing: Timing) -> Due {
        let mut due = Due::default();

        let last_pressed = self.last_pressed;
        self.keys.retain_mut(|key| {
            if now.duration_since(key.pressed_at) >= MAX_HOLD {
                due.expired.push(*key);
                return false;
            }
            if Some(key.code) != last_pressed && key.next_fire <= now {
                key.next_fire = now + timing.interval;
                due.fire.push(key.code);
            }
            true
        });
        if due
            .expired
            .iter()
            .any(|key| Some(key.code) == self.last_pressed)
        {
            self.last_pressed = None;
        }

        due
    }

    fn next_wake(&self) -> Option<Instant> {
        self.keys
            .iter()
            .flat_map(|key| {
                let expiry = key.pressed_at + MAX_HOLD;
                let repeat = (Some(key.code) != self.last_pressed).then_some(key.next_fire);
                [Some(expiry), repeat]
            })
            .flatten()
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Repeat = Repeat { osd_kind: None };
    const TIMING: Timing = Timing {
        delay: Duration::from_millis(500),
        interval: Duration::from_millis(33),
    };

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn the_latest_key_is_left_to_windows() {
        let base = Instant::now();
        let mut held = Held::new();
        assert!(held.press(0x78, PLAIN, base, TIMING));

        let due = held.take_due(at(base, 5_000), TIMING);
        assert!(
            due.expired.is_empty() && due.fire.is_empty(),
            "a lone key is the one Windows repeats"
        );
    }

    #[test]
    fn a_key_stops_repeating_in_windows_when_another_is_pressed() {
        let base = Instant::now();
        let mut held = Held::new();
        held.press(0x78, PLAIN, base, TIMING);
        held.press(0x7a, PLAIN, at(base, 700), TIMING);

        let due = held.take_due(at(base, 800), TIMING);
        assert_eq!(due.fire, vec![0x78], "only the earlier key needs firing");
    }

    #[test]
    fn a_key_is_not_fired_again_before_its_interval() {
        let base = Instant::now();
        let mut held = Held::new();
        held.press(0x78, PLAIN, base, TIMING);
        held.press(0x7a, PLAIN, at(base, 100), TIMING);

        assert_eq!(held.take_due(at(base, 600), TIMING).fire, vec![0x78]);
        assert!(held.take_due(at(base, 610), TIMING).fire.is_empty());
        assert_eq!(held.take_due(at(base, 640), TIMING).fire, vec![0x78]);
    }

    #[test]
    fn a_fresh_key_waits_out_the_repeat_delay_before_it_is_fired() {
        let base = Instant::now();
        let mut held = Held::new();
        held.press(0x78, PLAIN, base, TIMING);
        held.press(0x7a, PLAIN, at(base, 10), TIMING);

        assert!(held.take_due(at(base, 400), TIMING).fire.is_empty());
        assert_eq!(held.take_due(at(base, 500), TIMING).fire, vec![0x78]);
    }

    #[test]
    fn the_remaining_key_takes_over_when_the_latest_is_released() {
        let base = Instant::now();
        let mut held = Held::new();
        held.press(0x78, PLAIN, base, TIMING);
        held.press(0x7a, PLAIN, at(base, 700), TIMING);
        held.release(0x7a);

        let due = held.take_due(at(base, 800), TIMING);
        assert_eq!(
            due.fire,
            vec![0x78],
            "Windows will not resume the older key"
        );
    }

    #[test]
    fn two_keys_held_together_are_both_alive_but_only_one_is_fired() {
        let base = Instant::now();
        let mut held = Held::new();
        held.press(0x78, PLAIN, base, TIMING);
        held.press(0x7a, PLAIN, at(base, 700), TIMING);

        // Windows repeating the later key refreshes it; the earlier one is ours.
        assert!(!held.press(0x7a, PLAIN, at(base, 1_200), TIMING));
        let due = held.take_due(at(base, 1_210), TIMING);
        assert_eq!(due.fire, vec![0x78]);
    }

    #[test]
    fn a_repeat_of_a_held_key_is_not_a_fresh_press() {
        let base = Instant::now();
        let mut held = Held::new();
        assert!(held.press(0x78, PLAIN, base, TIMING));
        assert!(!held.press(0x78, PLAIN, at(base, 40), TIMING));
        assert_eq!(held.keys.len(), 1);
    }

    #[test]
    fn a_repeat_of_the_older_key_does_not_steal_the_latest_slot() {
        let base = Instant::now();
        let mut held = Held::new();
        held.press(0x78, PLAIN, base, TIMING);
        held.press(0x7a, PLAIN, at(base, 100), TIMING);
        held.press(0x78, PLAIN, at(base, 130), TIMING);

        assert_eq!(held.last_pressed, Some(0x7a));
    }

    #[test]
    fn a_release_that_was_never_seen_is_given_up_on() {
        let base = Instant::now();
        let mut held = Held::new();
        held.press(0x78, PLAIN, base, TIMING);
        held.press(0x7a, PLAIN, at(base, 10), TIMING);

        let due = held.take_due(base + MAX_HOLD + Duration::from_millis(20), TIMING);
        assert_eq!(due.expired.len(), 2);
        assert!(due.fire.is_empty());
        assert!(held.keys.is_empty());
        assert_eq!(held.last_pressed, None);
    }

    #[test]
    fn the_thread_sleeps_until_the_next_key_is_due() {
        let base = Instant::now();
        let mut held = Held::new();
        assert_eq!(held.next_wake(), None);

        held.press(0x78, PLAIN, base, TIMING);
        held.press(0x7a, PLAIN, at(base, 100), TIMING);
        assert_eq!(
            held.next_wake(),
            Some(at(base, 500)),
            "the earlier key fires after its delay; the later is Windows's"
        );
    }

    #[test]
    fn releasing_an_unheld_key_is_harmless() {
        let mut held = Held::new();
        assert!(held.release(0x78).is_none());
    }

    #[test]
    fn timing_follows_the_control_panel_scale() {
        let fastest = Timing::from_settings(0, 31);
        assert_eq!(fastest.delay, Duration::from_millis(250));
        assert!((fastest.interval.as_secs_f64() - 1.0 / 30.0).abs() < 0.0005);

        let slowest = Timing::from_settings(3, 0);
        assert_eq!(slowest.delay, Duration::from_millis(1000));
        assert!((slowest.interval.as_secs_f64() - 0.4).abs() < 0.0005);

        // A value outside the documented range must not run away.
        assert_eq!(Timing::from_settings(99, 99), Timing::from_settings(3, 31));
    }
}
