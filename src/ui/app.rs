#[macro_export]
macro_rules! disable_osd {
    ($($body:tt)*) => {{
        let _osd_disable_guard = $crate::ui::app::OsdDisableGuard::new();
        { $($body)* }
    }};
}

include!("app/app_context.rs");
include!("app/app_core.rs");
include!("app/runtime.rs");
include!("app/settings_process.rs");
