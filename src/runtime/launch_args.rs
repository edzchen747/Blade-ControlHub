pub const SETTINGS_MODE_ARG: &str = "--settings";

pub fn current_process_is_settings_mode() -> bool {
    std::env::args().any(is_settings_mode_arg)
}

pub fn is_settings_mode_arg(arg: impl AsRef<str>) -> bool {
    arg.as_ref().eq_ignore_ascii_case(SETTINGS_MODE_ARG)
}
