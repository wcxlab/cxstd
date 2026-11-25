//! Windows through `cxstd::platform::create_window().handler(handler).build()`.
//! Client sizes and input coordinates use logical pixels; GPU frames use physical pixels.
//! High pixel density is enabled automatically. Window properties are cached and
//! refreshed before update callbacks and when window metrics change.
//! Redraw and close requests are queued events addressed to a window.
//! Native decorations handle titlebar movement and resizing. Resizable borderless
//! windows use internal edge hit testing; client content is never a drag region.
//! Platform owns each window's handler and presentation surface. GPU handles are shared
//! through `cxstd::platform::gpu()`.
//! Window references are cloneable. Callers drive `update()` and `render()` themselves.

mod traits;
mod types;
mod window;

pub use traits::{WindowHandlerTrait, WindowTrait};
pub use types::*;
pub(super) use window::WindowState;
pub use window::{Window, WindowBuilder, WindowFrame, WindowRef};
