//! Borrowed references and shared input identifiers exposed at the crate root.

mod keycode;
mod reference;

pub use keycode::{GamepadAxis, KeyCode, KeyLocation};
pub use reference::{Ref, RefMut};
