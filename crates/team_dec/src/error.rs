use thiserror::Error;

#[derive(Debug, Error)]
pub enum TeamDetectorError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Failed to parse data: {0}")]
    Parse(String),

    #[error("Rate limit exceeded or failed to acquire permit")]
    RateLimit,

    #[error("Required data not found: {0}")]
    NotFound(String),

    #[error("Invalid configuration: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, TeamDetectorError>;
