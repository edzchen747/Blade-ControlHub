use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

use tracing::{error, info, warn};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, MSG, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::core::shared_state::{ALT_PRESSED, SHIFT_PRESSED};
use crate::win::input::{key_map::KEY_MAP, vkey};

const VK_SHIFT: u8 = 0x10;
const VK_LSHIFT: u8 = 0xA0;
const VK_RSHIFT: u8 = 0xA1;
const VK_MENU: u8 = 0x12;
const LEFT_SHIFT_MASK: u8 = 0b01;
const RIGHT_SHIFT_MASK: u8 = 0b10;

pub struct KeyHook {}

static KEY_HOOK_RUNNING: AtomicBool = AtomicBool::new(false);
static KEY_HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static KEY_HOOK_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static SHIFT_KEYS_DOWN: AtomicU8 = AtomicU8::new(0);

impl KeyHook {
    pub fn start() {
        join_finished_key_hook_thread();

        if KEY_HOOK_RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }

        match thread::Builder::new()
            .name("blade-keyboard-hook".to_string())
            .spawn(run_keyboard_hook)
        {
            Ok(handle) => *key_hook_thread() = Some(handle),
            Err(error) => {
                KEY_HOOK_RUNNING.store(false, Ordering::SeqCst);
                error!(%error, "Failed to start keyboard hook thread");
            }
        }
    }

    pub fn stop() {
        KEY_HOOK_RUNNING.store(false, Ordering::SeqCst);
        ALT_PRESSED.store(false, Ordering::SeqCst);
        SHIFT_PRESSED.store(false, Ordering::SeqCst);
        SHIFT_KEYS_DOWN.store(0, Ordering::SeqCst);

        let thread_id = KEY_HOOK_THREAD_ID.load(Ordering::SeqCst);
        if thread_id != 0 {
            let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        }
        join_key_hook_thread();
    }
}

fn key_hook_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    KEY_HOOK_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_finished_key_hook_thread() {
    let should_join = key_hook_thread()
        .as_ref()
        .is_some_and(JoinHandle::is_finished);

    if should_join {
        join_key_hook_thread();
    }
}

fn join_key_hook_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = key_hook_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current keyboard hook thread during shutdown");
        *key_hook_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("Keyboard hook thread panicked during shutdown");
    }
}

fn run_keyboard_hook() {
    let thread_id = unsafe { GetCurrentThreadId() };
    KEY_HOOK_THREAD_ID.store(thread_id, Ordering::SeqCst);

    let hook = match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_hook_proc), None, 0) } {
        Ok(hook) => hook,
        Err(error) => {
            KEY_HOOK_RUNNING.store(false, Ordering::SeqCst);
            KEY_HOOK_THREAD_ID.store(0, Ordering::SeqCst);
            error!(?error, "Failed to install keyboard hook");
            return;
        }
    };

    if !KEY_HOOK_RUNNING.load(Ordering::SeqCst) {
        let _ = unsafe { UnhookWindowsHookEx(hook) };
        KEY_HOOK_THREAD_ID.store(0, Ordering::SeqCst);
        return;
    }

    info!("Keyboard hook thread started");
    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {}
        if let Err(error) = UnhookWindowsHookEx(hook) {
            warn!(?error, "Failed to remove keyboard hook");
        }
    }
    KEY_HOOK_THREAD_ID.store(0, Ordering::SeqCst);
    KEY_HOOK_RUNNING.store(false, Ordering::SeqCst);
    info!("Keyboard hook thread stopped");
}

unsafe extern "system" fn key_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam.0 != 0 {
        let keyboard = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let key_code = keyboard.vkCode as u8;
        let block = match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => handle_key_event(key_code, true),
            WM_KEYUP | WM_SYSKEYUP => {
                handle_key_event(key_code, false);
                false
            }
            _ => false,
        };
        if block {
            return LRESULT(1);
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn handle_key_event(key_code: u8, pressed: bool) -> bool {
    if !KEY_HOOK_RUNNING.load(Ordering::SeqCst) {
        return false;
    }

    match key_code {
        VK_MENU => ALT_PRESSED.store(pressed, Ordering::SeqCst),
        VK_LSHIFT => update_shift_state(LEFT_SHIFT_MASK, pressed),
        VK_RSHIFT => update_shift_state(RIGHT_SHIFT_MASK, pressed),
        VK_SHIFT => {
            SHIFT_KEYS_DOWN.store(if pressed { u8::MAX } else { 0 }, Ordering::SeqCst);
            SHIFT_PRESSED.store(pressed, Ordering::SeqCst);
        }
        _ => {}
    }

    pressed
        && KEY_MAP
            .get(&vkey::Key::from(key_code).into())
            .is_some_and(|event_action| event_action.execute())
}

fn update_shift_state(mask: u8, pressed: bool) {
    let previous = if pressed {
        SHIFT_KEYS_DOWN.fetch_or(mask, Ordering::SeqCst)
    } else {
        SHIFT_KEYS_DOWN.fetch_and(!mask, Ordering::SeqCst)
    };
    let current = if pressed {
        previous | mask
    } else {
        previous & !mask
    };
    SHIFT_PRESSED.store(current != 0, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_clears_modifier_state() {
        ALT_PRESSED.store(true, Ordering::SeqCst);
        SHIFT_PRESSED.store(true, Ordering::SeqCst);
        KEY_HOOK_RUNNING.store(true, Ordering::SeqCst);

        KeyHook::stop();

        assert!(!KEY_HOOK_RUNNING.load(Ordering::SeqCst));
        assert!(!ALT_PRESSED.load(Ordering::SeqCst));
        assert!(!SHIFT_PRESSED.load(Ordering::SeqCst));
    }

    #[test]
    fn stopped_hook_does_not_handle_keys() {
        KEY_HOOK_RUNNING.store(false, Ordering::SeqCst);

        assert!(!handle_key_event(VK_MENU, true));
        assert!(!ALT_PRESSED.load(Ordering::SeqCst));
    }

    #[test]
    fn left_and_right_shift_keys_set_the_cycle_reverse_modifier() {
        SHIFT_KEYS_DOWN.store(0, Ordering::SeqCst);
        SHIFT_PRESSED.store(false, Ordering::SeqCst);

        update_shift_state(LEFT_SHIFT_MASK, true);
        assert!(SHIFT_PRESSED.load(Ordering::SeqCst));

        update_shift_state(RIGHT_SHIFT_MASK, true);
        update_shift_state(LEFT_SHIFT_MASK, false);
        assert!(SHIFT_PRESSED.load(Ordering::SeqCst));

        update_shift_state(RIGHT_SHIFT_MASK, false);
        assert!(!SHIFT_PRESSED.load(Ordering::SeqCst));
    }
}
