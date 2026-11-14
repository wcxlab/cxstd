//! Windows through `platform.window().wgpu_window().handler(handler).build()`.
//! Client sizes and input coordinates use logical pixels; GPU frames use physical pixels.
//! High pixel density is enabled automatically. Window properties are cached and
//! refreshed before update callbacks and when window metrics change.
//! Redraw and close requests are queued events addressed to a window.
//! Native decorations handle titlebar movement and resizing. Resizable borderless
//! windows use internal edge hit testing; client content is never a drag region.
//! Each window owns its handler and GPU presentation resources.
//! Platform manages initialization and native event polling. Windows retain
//! the shared native context. Callers drive `update()` and `render()` themselves.

mod subsystem;
mod types;
mod wgpu_window;
mod window;

pub use subsystem::WindowSubsystem;
pub use types::*;
pub use wgpu_window::{WgpuWindow, WgpuWindowBuilder, WgpuWindowFrame};
pub use window::{Window, WindowHandler};
