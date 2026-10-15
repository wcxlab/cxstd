//! Window contracts with optional `sdl::init()?.create_window(...)` and
//! `sdl::init()?.create_wgpu_window(...)` backends.

mod backends;
mod error;
mod event;
mod flags;
mod handler;
mod types;
mod window;

#[cfg(feature = "sdl")]
pub use backends::sdl;
pub use error::WindowError;
pub use event::*;
pub use flags::WindowFlags;
pub use handler::WindowHandler;
pub use types::*;
pub use window::Window;
#[cfg(feature = "wgpu")]
pub use window::{WgpuFrame, WgpuWindow};
