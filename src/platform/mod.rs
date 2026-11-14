//! Native capabilities through `Platform::new()`.
//! Create windows with `platform.window().wgpu_window()` and enumerate devices with
//! `platform.gamepad().get_gamepads()`. Call `platform.event().poll_event()` to
//! receive events and update input state. Only one event subsystem can own a native context.
//! Windows, builders and gamepads retain the shared context until they are dropped.
//! All native operations stay on the platform thread; SDL types remain private.

pub(crate) mod backend;
pub mod event;
mod events;
pub mod gamepad;
pub mod window;

pub use event::{EventSubsystem, poll_event};
pub use events::*;
pub use gamepad::GamepadSubsystem;
pub use window::WindowSubsystem;

use crate::Result;

pub struct Platform {
    window: WindowSubsystem,
    gamepad: GamepadSubsystem,
    event: EventSubsystem,
}

impl Platform {
    pub fn new() -> Result<Self> {
        let context = backend::sdl::init()?.context;
        let event = EventSubsystem::new(context.clone())?;
        Ok(Self {
            window: WindowSubsystem {
                context: context.clone(),
            },
            gamepad: GamepadSubsystem { context },
            event,
        })
    }

    pub fn window(&self) -> &WindowSubsystem {
        &self.window
    }

    pub fn gamepad(&self) -> &GamepadSubsystem {
        &self.gamepad
    }

    pub fn event(&mut self) -> &mut EventSubsystem {
        &mut self.event
    }
}
