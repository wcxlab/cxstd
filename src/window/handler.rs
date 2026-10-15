//! Host callbacks shared by native and GPU windows through `WindowHandler<W>`.

use super::{Window, WindowError, WindowEvent};

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

    fn window_event(&mut self, _window: &mut W, _event: &WindowEvent) -> Result<(), WindowError> {
        Ok(())
    }

    fn frame_ready(&mut self, _window: &mut W) -> Result<(), WindowError> {
        Ok(())
    }
}
