//! Shared window operations and host lifecycle callbacks.
//! State queries report cached native flags; hidden windows may report pending changes.

use super::{WindowCursor, WindowError, WindowHandle, WindowId};
use crate::platform::PlatformEventMessage;

/// Native window operations shared by hosts.
pub trait Window {
    fn id(&self) -> WindowId;

    /// Content width in logical pixels, excluding native decorations.
    fn inner_width(&self) -> f32;

    /// Content height in logical pixels, excluding native decorations.
    fn inner_height(&self) -> f32;

    /// Content width in physical pixels, excluding native decorations.
    fn inner_physical_width(&self) -> u32;

    /// Content height in physical pixels, excluding native decorations.
    fn inner_physical_height(&self) -> u32;

    /// Horizontal screen position in logical pixels at the window's current scale.
    fn screen_x(&self) -> f32;

    /// Vertical screen position in logical pixels at the window's current scale.
    fn screen_y(&self) -> f32;

    /// Horizontal screen position in physical pixels at the window's current density.
    fn screen_physical_x(&self) -> i32;

    /// Vertical screen position in physical pixels at the window's current density.
    fn screen_physical_y(&self) -> i32;

    /// Physical pixels per logical pixel.
    fn device_pixel_ratio(&self) -> f32;

    fn is_maximized(&self) -> bool;

    fn is_minimized(&self) -> bool;

    fn is_fullscreen(&self) -> bool;

    fn is_borderless(&self) -> bool;

    /// Whether this window currently has keyboard input focus.
    fn is_focused(&self) -> bool;

    /// Queue a Redraw event for the next event poll.
    fn request_redraw(&mut self) -> Result<(), WindowError>;

    /// Queue a Close event; the host decides when to close.
    fn request_close(&mut self) -> Result<(), WindowError>;

    /// Request a client size in logical pixels.
    fn resize_to(&mut self, width: i32, height: i32) -> Result<(), WindowError>;

    /// Request a client size change in logical pixels.
    fn resize_by(&mut self, width: i32, height: i32) -> Result<(), WindowError>;

    /// Request a position in platform desktop coordinates, matching the builder.
    fn move_to(&mut self, x: i32, y: i32) -> Result<(), WindowError>;

    /// Request a position change in platform desktop coordinates.
    fn move_by(&mut self, x: i32, y: i32) -> Result<(), WindowError>;

    /// Request input focus and bring the window forward.
    fn focus(&mut self) -> Result<(), WindowError>;

    fn maximize(&mut self) -> Result<(), WindowError>;

    fn minimize(&mut self) -> Result<(), WindowError>;

    fn restore(&mut self) -> Result<(), WindowError>;

    /// Request entering or leaving fullscreen.
    fn fullscreen(&mut self, fullscreen: bool) -> Result<(), WindowError>;

    /// Enable or disable a borderless window.
    fn borderless(&mut self, borderless: bool) -> Result<(), WindowError>;

    fn set_cursor(&mut self, cursor: WindowCursor) -> Result<(), WindowError>;

    fn start_text_input(&mut self) -> Result<(), WindowError>;

    fn stop_text_input(&mut self) -> Result<(), WindowError>;

    fn raw_window_handle(&self) -> Result<WindowHandle<'_>, WindowError>;
}

/// Host callbacks for window initialization, events, updates, and rendering.
/// Each window owns its handler and calls `init` once when built.
pub trait WindowHandler<W: Window + ?Sized> {
    fn error(&mut self, error: WindowError) -> Result<(), WindowError> {
        Err(error)
    }

    fn init(&mut self, _window: &mut W) -> Result<(), WindowError> {
        Ok(())
    }

    fn update(&mut self, _window: &mut W) -> Result<(), WindowError> {
        Ok(())
    }

    fn render(&mut self, _window: &mut W) -> Result<(), WindowError> {
        Ok(())
    }

    fn window_event(
        &mut self,
        _window: &mut W,
        _event: &PlatformEventMessage,
    ) -> Result<(), WindowError> {
        Ok(())
    }

    fn frame_ready(&mut self, _window: &mut W) -> Result<(), WindowError> {
        Ok(())
    }
}

impl<W: Window + ?Sized> WindowHandler<W> for () {}
