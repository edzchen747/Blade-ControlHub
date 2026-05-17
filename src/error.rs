use std::fmt;

#[derive(Debug)]
pub enum AppError {
    HardwareTimeout,
    HardwareResponseInvalid,
    ConfigParse(String),
    ConfigSerialize(String),
    AudioEndpointUnavailable,
    DisplayNotFound,
    DeviceNotFound,
    ChannelDisconnected,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::HardwareTimeout => write!(f, "Hardware device did not respond in time"),
            AppError::HardwareResponseInvalid => {
                write!(f, "Hardware returned an unexpected response")
            }
            AppError::ConfigParse(msg) => write!(f, "Failed to parse config: {}", msg),
            AppError::ConfigSerialize(msg) => write!(f, "Failed to serialize config: {}", msg),
            AppError::AudioEndpointUnavailable => write!(f, "Audio endpoint is not available"),
            AppError::DisplayNotFound => write!(f, "No compatible display device found"),
            AppError::DeviceNotFound => write!(f, "No compatible Razer device found"),
            AppError::ChannelDisconnected => write!(f, "Internal message channel disconnected"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::ConfigParse(err.to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(_err: anyhow::Error) -> Self {
        AppError::HardwareTimeout
    }
}

pub type AppResult<T> = Result<T, AppError>;
