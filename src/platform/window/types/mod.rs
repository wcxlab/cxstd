//! Window cursor, identity, native handles and errors exposed through `cxstd::platform::window`.

mod cursor;
mod error;
mod window;

pub use cursor::WindowCursor;
pub use error::WindowError;
pub use window::{WindowHandle, WindowId};
