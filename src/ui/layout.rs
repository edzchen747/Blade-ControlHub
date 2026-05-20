//! Layout dimensions, spacing tokens, and viewport boundaries.
//!
//! All structural measurements (window sizes, margins, padding, spacing,
//! rounding, animation timing) live here. Color palettes and typography
//! remain in `theme`.

use eframe::egui::Vec2;

// ── OSD Layout ───────────────────────────────────────────────────────────────

/// OSD overlay window dimensions.
pub const OSD_WINDOW_SIZE: Vec2 = Vec2::new(220.0, 220.0);

/// How long the OSD stays fully visible before fading out (milliseconds).
pub const OSD_DISPLAY_DURATION_MS: u64 = 1500;

/// Vertical position ratio of the OSD window relative to screen height.
/// `0.85` places the OSD near the bottom-center of the viewport.
pub const OSD_POSITION_Y_RATIO: f32 = 0.85;

/// Corner rounding radius for the OSD panel.
pub const OSD_ROUNDING: f32 = 12.0;

/// Border stroke width around the OSD panel.
pub const OSD_BORDER_WIDTH: f32 = 2.0;

/// Outer margin (padding between window edge and OSD panel).
pub const OSD_OUTER_MARGIN: f32 = 10.0;

/// Inner margin (padding inside the OSD panel).
pub const OSD_INNER_MARGIN: f32 = 10.0;

/// Width of the segmented slider bar.
pub const SLIDER_BAR_WIDTH: f32 = 160.0;

/// Height of each slider segment.
pub const SLIDER_BAR_HEIGHT: f32 = 8.0;

/// Spacing between slider segments.
pub const SLIDER_BAR_SPACING: f32 = 2.0;

/// Maximum rendered size for OSD icons.
pub const ICON_MAX_SIZE: Vec2 = Vec2::new(80.0, 80.0);

/// Font size for OSD text labels.
pub const OSD_TEXT_FONT_SIZE: f32 = 27.0;

/// Fade-in speed multiplier (alpha units per second).
pub const FADE_IN_SPEED: f32 = 255.0;

/// Fade-out speed multiplier (alpha units per second).
pub const FADE_OUT_SPEED: f32 = 2.0;

/// Epsilon threshold for fade animation completion.
pub const FADE_EPSILON: f32 = 0.001;

/// Target alpha when the OSD is fully visible.
pub const TARGET_ALPHA_VISIBLE: f32 = 0.9;

/// Spacing above the OSD icon.
pub const OSD_ICON_TOP_SPACING: f32 = 20.0;

/// Spacing between icon and text in the OSD.
pub const OSD_ICON_TEXT_SPACING: f32 = 8.0;

/// Spacing at the top of the OSD content area.
pub const OSD_CONTENT_TOP_SPACING: f32 = 10.0;

/// Vertical space reserved when no icon is present.
pub const OSD_PLACEHOLDER_ICON_HEIGHT: f32 = 60.0;

/// Vertical space reserved when no text is present.
pub const OSD_PLACEHOLDER_TEXT_HEIGHT: f32 = 32.0;

/// Inactive slider segment alpha multiplier (very dim).
pub const SLIDER_INACTIVE_ALPHA: f32 = 0.02;

// ── Tray Layout ──────────────────────────────────────────────────────────────

/// Rendered pixel size for the system tray icon.
pub const TRAY_ICON_SIZE: u32 = 64;

/// Scale factor applied to the tray icon SVG during rasterization.
pub const TRAY_ICON_SCALE_FACTOR: f32 = 1.2;

// ── Settings Window Layout ───────────────────────────────────────────────────

/// Fixed inner size of the settings window.
pub const SETTINGS_WINDOW_SIZE: Vec2 = Vec2::new(450.0, 600.0);

/// Ratio of screen height used for bottom-right padding of the settings window.
pub const SETTINGS_PADDING_RATIO: f32 = 0.1;

/// Spacing before the settings tab content area.
pub const SETTINGS_CONTENT_TOP_SPACING: f32 = 20.0;

/// Desired width for text edit fields in the key mapping table.
pub const SETTINGS_TEXT_EDIT_WIDTH: f32 = 120.0;

/// Fixed width for the key-code button in key mapping rows.
pub const SETTINGS_KEY_BUTTON_WIDTH: f32 = 60.0;

/// Fixed height for the key-code button in key mapping rows.
pub const SETTINGS_KEY_BUTTON_HEIGHT: f32 = 20.0;

/// Vertical spacing between key mapping rows.
pub const SETTINGS_ROW_SPACING: f32 = 10.0;

/// Repaint interval while listening for a key press (milliseconds).
pub const SETTINGS_KEY_LISTEN_INTERVAL_MS: u64 = 200;

/// Icon pixel size for the settings window.
pub const SETTINGS_ICON_SIZE: u32 = 64;
