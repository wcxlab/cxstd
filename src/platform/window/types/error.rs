//! Errors returned by native window operations.

#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("window operation failed: {0}")]
    Nul(String),
    #[error("window operation is unsupported: {0}")]
    Unsupported(&'static str),
    #[error("unknown window error")]
    Unknown,
    #[error("the window is closed")]
    Closed,
    #[error("window callback failed: {0}")]
    Callback(#[source] anyhow::Error),
}
