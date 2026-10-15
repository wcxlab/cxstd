//! Window contracts with optional `sdl::init()?.create_window(handler, options)` and
//! `sdl::init()?.create_wgpu_window(handler, options)` backends.
//! `Window::request_redraw()` and `Window::request_close()` queue coalesced requests;
//! the host consumes them with `take_redraw_request()` and `take_close_request()`.

mod backends;
mod error;
mod event;
mod flags;
mod types;
mod window;

#[cfg(feature = "sdl")]
pub use backends::sdl;
pub use error::WindowError;
pub use event::*;
pub use flags::WindowFlags;
pub use types::*;
#[cfg(feature = "wgpu")]
pub use window::{WgpuFrame, WgpuWindow};
pub use window::{Window, WindowHandler};
