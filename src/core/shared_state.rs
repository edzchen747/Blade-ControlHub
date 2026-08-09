use std::sync::{
    OnceLock,
    atomic::{AtomicBool, AtomicI32, AtomicU8},
};

pub static IS_PLUGGED_IN: AtomicBool = AtomicBool::new(false);

pub static FN_PRESSED: AtomicBool = AtomicBool::new(false);
pub static ALT_PRESSED: AtomicBool = AtomicBool::new(false);
pub static SHIFT_PRESSED: AtomicBool = AtomicBool::new(false);
pub static PRIMARY_MULTIMEDIA_KEYS: AtomicBool = AtomicBool::new(false);

pub static KEYMAP_LISTENING: AtomicBool = AtomicBool::new(false);
pub static COMMAND_LAB_CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);

pub static SCREEN_ADJUSTING: AtomicI32 = AtomicI32::new(0);
pub static SCREEN_TARGET_LVL: AtomicU8 = AtomicU8::new(100);

pub static DEVICE_PIDS: OnceLock<Vec<u16>> = OnceLock::new();
