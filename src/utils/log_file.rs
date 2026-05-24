use std::io::{BufRead, BufReader, Write};
use tracing::info;
use tracing_subscriber::EnvFilter;

pub const LOG_PATH: &str = "app.log";
pub const TRUNC_MAX_LINES: usize = 1000;

pub fn set_cwd() {
    let exe_path = std::env::current_exe()
        .map_err(crate::error::AppError::Io)
        .unwrap();
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| {
            crate::error::AppError::Internal("Executable has no parent directory".to_string())
        })
        .unwrap();
    std::env::set_current_dir(exe_dir)
        .map_err(crate::error::AppError::Io)
        .unwrap();
}

pub fn init_log_file_writer() {
    truncate_log_file();

    let builder = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug")),
        )
        .with_target(false)
        .compact();

    let _guard;

    if cfg!(debug_assertions) {
        builder.init();
    } else {
        let file_appender = tracing_appender::rolling::never(".", LOG_PATH);

        let (file_writer, g) = tracing_appender::non_blocking(file_appender);
        _guard = Some(g);

        builder.with_writer(file_writer).init();
    }

    info!("── Start New Session ───────────────────────────────────────────────────────");
}

pub fn truncate_log_file() {
    let file = match std::fs::OpenOptions::new().read(true).open(LOG_PATH) {
        Ok(f) => f,
        Err(_) => return,
    };

    let reader = BufReader::new(file);
    let mut lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();

    if lines.len() > TRUNC_MAX_LINES {
        lines = lines.split_off(lines.len() - TRUNC_MAX_LINES);
    } else {
        return;
    }

    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(LOG_PATH)
    {
        Ok(f) => f,
        Err(_) => return,
    };

    for line in lines {
        let _ = writeln!(file, "{}", line);
    }
}
