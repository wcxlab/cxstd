//! Gamepad discovery through the owning platform's shared context.

use std::rc::Rc;

use crate::platform::gamepad::Gamepad;
use crate::platform::backend::sdl::{self, SdlContext};

#[derive(Clone)]
pub struct GamepadService {
    pub(in crate::platform) context: Rc<SdlContext>,
}

impl GamepadService {
    /// Return the current standard gamepad slots, including vacant slots.
    pub fn get_gamepads(&self) -> anyhow::Result<Vec<Option<Gamepad>>> {
        Ok(sdl::gamepads_for_context(&self.context))
    }

    /// Return the current device in the slot; vacant or invalid indices return None.
    /// Disconnected slots may be reused by later connections.
    pub fn get_gamepad(&self, index: u32) -> Option<Gamepad> {
        sdl::gamepad_for_index(&self.context, index)
    }
}
