use std::io::{BufRead, BufReader, Write};
use std::sync::OnceLock;
use tracing::info;
use tracing_subscriber::EnvFilter;

pub const LOG_PATH: &str = "app.log";
pub const TRUNC_MAX_LINES: usize = 1000;
static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

pub fn set_cwd() -> crate::error::AppResult<()> {
    let exe_path = std::env::current_exe().map_err(crate::error::AppError::Io)?;
    let exe_dir = exe_path.parent().ok_or_else(|| {
        crate::error::AppError::Internal("Executable has no parent directory".to_string())
    })?;
    std::env::set_current_dir(exe_dir).map_err(crate::error::AppError::Io)?;
    Ok(())
}

pub fn init_log_file_writer() {
    truncate_log_file();

    let builder = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug")),
        )
        .with_target(false)
        .compact();

    if cfg!(debug_assertions) {
        let _ = builder.try_init();
    } else {
        let file_appender = tracing_appender::rolling::never(".", LOG_PATH);

        let (file_writer, g) = tracing_appender::non_blocking(file_appender);
        let _ = LOG_GUARD.set(g);

        let _ = builder.with_writer(file_writer).try_init();
    }

    info!("── Start New Session ───────────────────────────────────────────────────────");
}

pub fn truncate_log_file() {
    let file = match std::fs::OpenOptions::new().read(true).open(LOG_PATH) {
        Ok(f) => f,
        Err(_) => return,
    };

    let reader = BufReader::new(file);
    let mut lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

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
