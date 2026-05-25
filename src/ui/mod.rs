// ─── Public UI API ────────────────────────────────────────────────────────────
//
// This module serves as the single entry-point for the entire UI layer.
// All external consumers (main.rs, win/, config/) should import through here.
//
// Architecture:
//   app            — Tauri window orchestration
//   tray           — system tray icon + menu
//   osd            — on-screen display animations
//   settings       — settings window UI (includes tabs, store)
//   event_dispatcher — cross-thread event routing
//   theme          — colors, typography, timing constants
//   layout         — viewport dimensions and spacing
//   icons          — SVG icon loading utilities
//   custom_key_map — key mapping data types
//
// Internal modules (not exposed):
//   tray_icon_module, tray_menu                 — tray sub-modules
// ───────────────────────────────────────────────────────────────────────────────

pub mod app;
pub mod app_events;
pub mod custom_key_map;
pub mod event_dispatcher;
pub mod icons;
pub mod layout;
pub mod osd;
pub mod osd_animation;
pub mod settings;
pub mod theme;
pub mod tray;
mod tray_icon_module;
mod tray_menu;

/// Re-exports for external consumers.
/// Use `ui::prelude::*` to import all public UI types.
///
/// These are intentionally unused internally — they form the public API surface
/// that external consumers (future crates, tests, documentation) will import.
#[allow(unused_imports)]
pub mod prelude {
    // App orchestration
    pub use crate::ui::app::{AppContext, AppHandle, app};

    // Event types
    pub use crate::ui::app_events::{AppEvent, OsdEvent, OsdResponse};

    // Key mapping domain model
    pub use crate::ui::custom_key_map::{CustomKeyMap, FuncKeyMap, RazerKeyMap};

    // Event routing
    pub use crate::ui::event_dispatcher::EventDispatcher;

    // Icon registry
    pub use crate::ui::icons::OsdIconId;

    // Layout constants
    pub use crate::ui::layout::*;

    // On-screen display
    pub use crate::ui::osd::Osd;

    // Settings persistence
    pub use crate::ui::settings::store::SettingsStore;

    // Theme constants
    pub use crate::ui::theme::*;

    // Tray
    pub use crate::ui::tray::{build_tray_icon, set_perf_mode_icon};
}
