use std::time::Duration;

use sysinfo::System;
use tracing::{error, warn};

use crate::runtime::launch_args::is_settings_mode_arg;

const APP_PROCESS_NAMES: &[&str] = &["blade-controlhub", "blade-controlhub.exe"];
pub const SILENT_RESTART_CODE: i32 = 1;
const SILENT_RESTART_ARGS: &[&str] = &["--silent"];
const DEFAULT_RESTART_ARGS: &[&str] = &[];
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn restart_app(code: i32) -> ! {
    if let Err(e) = spawn_replacement_app(code) {
        error!(error = %e, "Failed to spawn replacement process");
    }
    std::process::exit(code);
}

pub fn spawn_replacement_app(code: i32) -> std::io::Result<()> {
    match std::env::current_exe() {
        Ok(current_exe) => spawn_replacement_process(&current_exe, code),
        Err(e) => Err(e),
    }
}

fn restart_args(code: i32) -> &'static [&'static str] {
    if code == SILENT_RESTART_CODE {
        SILENT_RESTART_ARGS
    } else {
        DEFAULT_RESTART_ARGS
    }
}

fn spawn_replacement_process(current_exe: &std::path::Path, code: i32) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;

    let mut cmd = std::process::Command::new(current_exe);
    cmd.args(restart_args(code))
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);

    cmd.spawn().map(drop)
}

pub fn close_running_instances() {
    let current_pid = match sysinfo::get_current_pid() {
        Ok(pid) => pid,
        Err(e) => {
            error!(error = ?e, "Failed to resolve current process PID; skipping instance cleanup");
            return;
        }
    };

    let mut sys = System::new_all();
    sys.refresh_processes();

    let mut found_old = false;
    for (pid, process) in sys.processes() {
        if is_app_process_name(process.name())
            && !is_settings_mode_process(process)
            && *pid != current_pid
        {
            if process.kill() {
                found_old = true;
            } else {
                warn!(pid = ?pid, "Failed to terminate previous Blade ControlHub process");
            }
        }
    }

    if found_old {
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn is_app_process_name(name: &str) -> bool {
    APP_PROCESS_NAMES
        .iter()
        .any(|app_name| name.eq_ignore_ascii_case(app_name))
}

fn is_settings_mode_process(process: &sysinfo::Process) -> bool {
    process
        .cmd()
        .iter()
        .any(|arg| is_settings_mode_arg(arg.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_code_one_uses_silent_startup() {
        assert_eq!(restart_args(SILENT_RESTART_CODE), ["--silent"]);
    }

    #[test]
    fn normal_restart_uses_no_extra_args() {
        assert!(restart_args(0).is_empty());
        assert!(restart_args(2).is_empty());
    }

    #[test]
    fn app_process_name_matches_sysinfo_name_with_or_without_extension() {
        assert!(is_app_process_name("blade-controlhub"));
        assert!(is_app_process_name("blade-controlhub.exe"));
        assert!(is_app_process_name("BLADE-CONTROLHUB.EXE"));
        assert!(!is_app_process_name("blade-settings"));
    }

    #[test]
    fn settings_mode_arg_is_detected_case_insensitively() {
        assert!(is_settings_mode_arg("--settings"));
        assert!(is_settings_mode_arg("--SETTINGS"));
        assert!(!is_settings_mode_arg("--silent"));
    }
}
