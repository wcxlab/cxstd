use std::{
    backtrace::Backtrace,
    fmt::{Debug, Display},
};

/// Log a message at the info level.
pub fn log(message: impl Display) {
    info(message);
}

pub fn debug(message: impl Display) {
    tracing::debug!("{message}");
}

pub fn info(message: impl Display) {
    tracing::info!("{message}");
}

pub fn warn(message: impl Display) {
    tracing::warn!("{message}");
}

pub fn error(message: impl Display) {
    tracing::error!("{message}");
}

/// Emit an assertion failure at the error level when the condition is false.
pub fn assert(condition: bool, message: impl Display) {
    if !condition {
        tracing::error!("Assertion failed: {message}");
    }
}

/// Log a message with the current call stack.
pub fn trace(message: impl Display) {
    tracing::info!(backtrace = %Backtrace::force_capture(), "{message}");
}

/// Log the debug representation of a value.
pub fn dir(value: impl Debug) {
    tracing::info!("{value:#?}");
}
