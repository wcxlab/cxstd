//! Core types exposed through `cxstd::Result`, `cxstd::Ref` and shared input identifiers.

mod keycode;
mod reference;
mod result;

pub use keycode::{GamepadAxis, KeyCode, KeyLocation};
pub use reference::{Ref, RefMut};
pub use result::Result;
