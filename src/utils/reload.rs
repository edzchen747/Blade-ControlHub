use std::time::Duration;

use sysinfo::System;

/// Restarts the application by spawning a new instance and exiting.
pub fn restart_app(code: i32) -> ! {
    let current_exe = std::env::current_exe().expect("Failed to get current exe path");
    std::process::Command::new(current_exe)
        .spawn()
        .expect("Failed to restart");
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
            eprintln!("Fatal internal error: {msg}");
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
            eprintln!("Fatal internal error: {msg} - {:?}", err);
            restart_app(1);
        })
    }
}

pub fn close_running_instances() {
    let mut sys = System::new_all();
    sys.refresh_processes();

    let current_pid = sysinfo::get_current_pid().expect("Failed to get app PID");
    let my_name = "hidapi.exe";

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
