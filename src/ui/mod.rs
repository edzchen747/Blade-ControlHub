pub mod app;
pub mod app_events;
pub mod command_lab;
pub mod custom_key_map;
pub mod event_dispatcher;
pub mod icons;
pub mod layout;
pub mod osd_controller;
pub mod settings;
pub mod settings_window;
pub mod theme;
pub mod tray;

#[allow(unused_imports)]
pub mod prelude {
    pub use crate::ui::app::{AppContext, app};

    pub use crate::ui::app_events::{AppEvent, OsdEvent};

    pub use crate::ui::custom_key_map::{CustomKeyMap, HypershiftKeyMap, RazerKeyMap};

    pub use crate::ui::command_lab::{CommandLab, CommandLabRow};

    pub use crate::ui::event_dispatcher::EventDispatcher;

    pub use crate::ui::icons::OsdIcon;

    pub use crate::ui::layout::*;

    pub use crate::ui::osd_controller::OsdController;

    pub use crate::ui::settings::store::SettingsStore;

    pub use crate::ui::theme::*;

    pub use crate::ui::tray::TrayManager;
}
