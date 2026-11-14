//! Native event polling through `platform.event().poll_event()`.
//! EventSubsystem owns exclusive polling and provides keyboard/mouse state queries.
//! Standalone native owners can use `poll_event()` without constructing Platform.

mod subsystem;

pub use subsystem::EventSubsystem;

use crate::platform::PlatformEventMessage;

/// Poll events for standalone windows and gamepads on their owning thread.
/// Returns None without a native owner. When a Platform owns the event subsystem,
/// poll through `platform.event().poll_event()` instead.
pub fn poll_event() -> Option<PlatformEventMessage> {
    crate::platform::backend::sdl::poll_event()
}
