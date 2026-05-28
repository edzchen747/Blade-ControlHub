/// Centralized atomic/shared state that is accessed across multiple modules.
///
/// This module consolidates all global atomic variables that need to be shared
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, AtomicI32, AtomicU8},
};

// ── Power State ──────────────────────────────────────────────────────────────
pub static IS_PLUGGED_IN: AtomicBool = AtomicBool::new(false);

// ── Keyboard / Input State ───────────────────────────────────────────────────
pub static FN_PRESSED: AtomicBool = AtomicBool::new(false);
pub static ALT_PRESSED: AtomicBool = AtomicBool::new(false);
pub static SHIFT_PRESSED: AtomicBool = AtomicBool::new(false);
pub static DEFAULT_MULTIMEDIA_KEYS: AtomicBool = AtomicBool::new(false);

// ── UI State ─────────────────────────────────────────────────────────────────
pub static KEYMAP_LISTENING: AtomicBool = AtomicBool::new(false);

// ── Display / Brightness State ───────────────────────────────────────────────
pub static SCREEN_ADJUSTING: AtomicI32 = AtomicI32::new(0);
pub static SCREEN_TARGET_LVL: AtomicU8 = AtomicU8::new(100);

// ── Razer Device State ───────────────────────────────────────────────────────
pub static DEVICE_PIDS: OnceLock<Vec<u16>> = OnceLock::new();
