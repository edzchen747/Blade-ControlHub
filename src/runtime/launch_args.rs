//! Command-line modes the executable can be launched in.
//!
//! The app is a single binary that also re-launches itself for one short-lived
//! helper: an elevated USB capture for Command Lab. Single-instance cleanup has
//! to know about that helper so it does not kill a capture that a still-running
//! parent is waiting on.

/// Runs one elevated USBPcap capture into the given file, then exits.
pub const COMMAND_LAB_CAPTURE_ARG: &str = "--command-lab-capture";

/// Suppresses the startup OSD, used by restarts.
pub const SILENT_ARG: &str = "--silent";

/// Arguments that mark a process as a helper rather than a main runtime.
const HELPER_MODE_ARGS: &[&str] = &[COMMAND_LAB_CAPTURE_ARG];

pub fn is_helper_mode_arg(arg: impl AsRef<str>) -> bool {
    let arg = arg.as_ref();
    HELPER_MODE_ARGS
        .iter()
        .any(|helper| arg.eq_ignore_ascii_case(helper))
}

/// The capture file path when this process was launched as a capture helper.
pub fn command_lab_capture_path() -> Option<std::path::PathBuf> {
    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if arg.eq_ignore_ascii_case(COMMAND_LAB_CAPTURE_ARG) {
            return args.next().map(std::path::PathBuf::from);
        }
    }
    None
}

pub fn is_silent_start() -> bool {
    std::env::args().any(|arg| arg.eq_ignore_ascii_case(SILENT_ARG))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_mode_args_are_matched_case_insensitively() {
        assert!(is_helper_mode_arg("--command-lab-capture"));
        assert!(is_helper_mode_arg("--COMMAND-LAB-CAPTURE"));
    }

    #[test]
    fn ordinary_runtime_flags_are_not_helper_modes() {
        assert!(!is_helper_mode_arg("--silent"));
        assert!(!is_helper_mode_arg("--debug"));
    }
}
