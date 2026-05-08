/// Restarts the application by spawning a new instance and exiting.
pub fn restart_app(code: i32) -> ! {
    let current_exe = std::env::current_exe().expect("Failed to get current exe path");
    std::process::Command::new(current_exe)
        .spawn()
        .expect("Failed to restart");
    std::process::exit(code);
}

/// Extension trait for `Option<T>` that restarts the app on `None` instead of panicking.
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
