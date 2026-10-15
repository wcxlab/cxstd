#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("window operation failed: {0}")]
    Nul(String),
    #[error("unknown window error")]
    Unknown,
}
