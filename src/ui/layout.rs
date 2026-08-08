use eframe::egui::Vec2;

pub const OSD_WINDOW_SIZE: Vec2 = Vec2::new(220.0, 220.0);

pub const OSD_DISPLAY_DURATION_MS: u64 = 1500;

pub const OSD_POSITION_Y_RATIO: f32 = 0.85;

pub const OSD_ROUNDING: f32 = 12.0;

pub const OSD_BORDER_WIDTH: f32 = 2.0;

pub const OSD_OUTER_MARGIN: f32 = 10.0;

pub const OSD_INNER_MARGIN: f32 = 10.0;

pub const SLIDER_BAR_WIDTH: f32 = 160.0;

pub const SLIDER_BAR_HEIGHT: f32 = 8.0;

pub const SLIDER_BAR_SPACING: f32 = 2.0;

pub const ICON_MAX_SIZE: Vec2 = Vec2::new(80.0, 80.0);

pub const OSD_TEXT_FONT_SIZE: f32 = 27.0;

pub const FADE_IN_SPEED: f32 = 255.0;

pub const FADE_OUT_SPEED: f32 = 2.0;

pub const TOTAL_ANIM_TIME_MS: f32 = OSD_DISPLAY_DURATION_MS as f32 + 800.0 / FADE_OUT_SPEED;

pub const FADE_EPSILON: f32 = 0.001;

pub const TARGET_ALPHA_VISIBLE: f32 = 0.9;

pub const OSD_ICON_TOP_SPACING: f32 = 20.0;

pub const OSD_ICON_TEXT_SPACING: f32 = 8.0;

pub const OSD_CONTENT_TOP_SPACING: f32 = 10.0;

pub const OSD_PLACEHOLDER_ICON_HEIGHT: f32 = 60.0;

pub const OSD_PLACEHOLDER_TEXT_HEIGHT: f32 = 32.0;

pub const SLIDER_INACTIVE_ALPHA: f32 = 0.02;

pub const TRAY_ICON_SIZE: u32 = 64;

pub const TRAY_ICON_SCALE_FACTOR: f32 = 1.2;

pub const SETTINGS_WINDOW_SIZE: Vec2 = Vec2::new(500.0, 750.0);

pub const SETTINGS_PADDING_RATIO: f32 = 0.1;

pub const SETTINGS_CONTENT_TOP_SPACING: f32 = 20.0;

pub const SETTINGS_TEXT_EDIT_WIDTH: f32 = 120.0;

pub const SETTINGS_KEY_BUTTON_WIDTH: f32 = 60.0;

pub const SETTINGS_KEY_BUTTON_HEIGHT: f32 = 20.0;

pub const SETTINGS_ROW_SPACING: f32 = 10.0;

pub const SETTINGS_KEY_LISTEN_INTERVAL_MS: u64 = 200;

pub const SETTINGS_ICON_SIZE: u32 = 64;
