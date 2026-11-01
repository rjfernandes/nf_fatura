use serde::Serialize;

/// Error carrying the status the HTTP API would answer with, and its message.
#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub status: u16,
    pub message: String,
}

pub type Result<T> = std::result::Result<T, AppError>;

impl AppError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(500, message)
    }
}

pub fn http_error(status: u16, message: impl Into<String>) -> AppError {
    AppError::new(status, message)
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

macro_rules! internal_from {
    ($($t:ty),*) => {$(
        impl From<$t> for AppError {
            fn from(e: $t) -> Self {
                AppError::internal(e.to_string())
            }
        }
    )*};
}
internal_from!(
    rusqlite::Error,
    reqwest::Error,
    std::io::Error,
    serde_json::Error,
    base64::DecodeError
);
