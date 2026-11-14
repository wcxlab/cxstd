//! Gamepad discovery through the owning platform's shared context.

use std::rc::Rc;

use crate::gamepad::Gamepad;
use crate::platform::backend::sdl::{self, SdlContext};
use anyhow::Result;

pub struct GamepadSubsystem {
    pub(in crate::platform) context: Rc<SdlContext>,
}

impl GamepadSubsystem {
    /// Enable device monitoring and enumerate standard slots, including vacant slots.
    pub fn get_gamepads(&self) -> Result<Vec<Option<Gamepad>>> {
        sdl::gamepads_for_context(&self.context)
    }

    /// Return the current device in the slot; vacant or invalid indices return None.
    /// Disconnected slots may be reused by later connections.
    pub fn get_gamepad(&self, index: u32) -> Option<Gamepad> {
        sdl::gamepad_for_index(&self.context, index)
    }
}
