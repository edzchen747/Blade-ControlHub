mod constants;
mod loader;
mod persist;
mod theme_color;

pub use constants::{CONFIG_PATH, RAZER_VID};
pub use loader::load_config;
pub use persist::persist_config;
pub use theme_color::ThemeColor;
