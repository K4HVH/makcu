#[derive(Debug, thiserror::Error)]
pub enum MakcuError {
    #[error("not connected")]
    NotConnected,
    #[error("port error: {0}")]
    Port(#[from] serialport::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("command timed out")]
    Timeout,
    #[error("device not found")]
    NotFound,
    #[error("disconnected")]
    Disconnected,
    #[error("protocol error: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, MakcuError>;
