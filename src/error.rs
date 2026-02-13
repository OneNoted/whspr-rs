use thiserror::Error;

#[derive(Error, Debug)]
pub enum WhsprError {
    #[error("audio error: {0}")]
    Audio(String),

    #[error("transcription error: {0}")]
    Transcription(String),

    #[error("injection error: {0}")]
    Injection(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("feedback error: {0}")]
    Feedback(String),

    #[error("download error: {0}")]
    Download(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, WhsprError>;
