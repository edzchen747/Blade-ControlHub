pub mod app;
pub mod app_events;
pub mod event_dispatcher;
pub mod icons;
pub mod osd_controller;
pub mod theme;
pub mod webui;

#[allow(unused_imports)]
pub mod prelude {
    pub use crate::ui::app::{AppContext, app};

    pub use crate::ui::app_events::{AppEvent, OsdEvent};

    pub use crate::ui::event_dispatcher::EventDispatcher;

    pub use crate::ui::icons::OsdIcon;

    pub use crate::ui::osd_controller::OsdController;

    pub use crate::ui::theme::*;
}
