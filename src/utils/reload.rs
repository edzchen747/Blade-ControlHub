use std::time::Duration;

use sysinfo::System;
use tracing::error;

/// Restarts the application by spawning a new instance and exiting.
pub fn restart_app(code: i32) -> ! {
    match std::env::current_exe() {
        Ok(current_exe) => {
            let mut cmd = std::process::Command::new(&current_exe);

            // Do not show startup OSD with code 1 restart
            if code == 1 {
                cmd.arg("--silent");
            }

            if let Err(e) = cmd.spawn() {
                error!(error = %e, "Failed to spawn restart process");
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to resolve current executable path for restart");
        }
    }
    std::process::exit(code);
}

/// Extension trait for `Option<T>` that restarts the app on `None` instead of panicking.
#[allow(dead_code)]
pub trait OptionReload<T> {
    fn or_reload(self, msg: &str) -> T;
}

impl<T> OptionReload<T> for Option<T> {
    fn or_reload(self, msg: &str) -> T {
        self.unwrap_or_else(|| {
            error!(
                message = msg,
                "Fatal: Option::None where value was required; restarting"
            );
            restart_app(1);
        })
    }
}

/// Extension trait for `Result<T, E>` that restarts the app on `Err` instead of panicking.
#[allow(dead_code)]
pub trait ResultReload<T, E> {
    fn or_reload(self, msg: &str) -> T;
}

impl<T, E: std::fmt::Debug> ResultReload<T, E> for Result<T, E> {
    fn or_reload(self, msg: &str) -> T {
        self.unwrap_or_else(|err| {
            error!(message = msg, error = ?err, "Fatal: Result::Err in non-recoverable path; restarting");
            restart_app(1);
        })
    }
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

    let my_name = "blade-controlhub.exe";

    let mut found_old = false;
    for (pid, process) in sys.processes() {
        if process.name() == my_name && *pid != current_pid {
            process.kill();
            found_old = true;
        }
    }

    if found_old {
        std::thread::sleep(Duration::from_millis(150));
    }
}
