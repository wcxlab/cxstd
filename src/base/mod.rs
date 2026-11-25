//! Shared input identifiers exposed at the crate root.

mod gamepad;
mod keycode;

pub use gamepad::{GamepadAxis, GamepadButtonCode};
pub use keycode::{KeyCode, KeyLocation};
