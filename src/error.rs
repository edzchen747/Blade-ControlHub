use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Hardware device did not respond within the timeout period")]
    HardwareTimeout,

    #[error("Hardware command {command:#06x} with args {args:?} failed after {attempts} attempts")]
    Protocol {
        command: u16,
        args: Vec<u8>,
        attempts: u8,
    },

    #[error("Hardware device disconnected")]
    HardwareDisconnected,

    #[error("Failed to initialise HID API: {0}")]
    HidApi(String),

    #[error("No Razer HID interfaces were accessible (all locked by OS or another process)")]
    NoInterfacesAccessible,

    #[error("Failed to parse configuration: {0}")]
    ConfigParse(String),

    #[error("Invalid internal state: {0}")]
    Internal(String),

    #[error("No compatible display device found")]
    DisplayNotFound,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::ConfigParse(err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
