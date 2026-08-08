pub const DEBUG_MODE_ARG: &str = "--debug";

pub fn is_enabled() -> bool {
    cfg!(debug_assertions) || std::env::args().any(is_debug_mode_arg)
}

pub fn is_debug_mode_arg(arg: impl AsRef<str>) -> bool {
    arg.as_ref().eq_ignore_ascii_case(DEBUG_MODE_ARG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_mode_flag_is_case_insensitive() {
        assert!(is_debug_mode_arg("--debug"));
        assert!(is_debug_mode_arg("--DEBUG"));
        assert!(!is_debug_mode_arg("--settings"));
    }
}
