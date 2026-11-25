mod backend;
pub mod clipboard;
pub mod console;
mod event;
pub mod gamepad;
mod gpu;
mod platform;
pub mod window;

pub use self::event::*;
pub use self::{clipboard::ClipboardService, gamepad::GamepadService, gpu::GpuService};
pub use self::platform::{clipboard, create_window, gamepad, gpu, init, poll_event};
