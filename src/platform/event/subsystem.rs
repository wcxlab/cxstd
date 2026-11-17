//! Exclusive native event polling and current keyboard/mouse state.

use std::rc::Rc;

use crate::KeyCode;
use crate::platform::PlatformEventMessage;
use crate::platform::backend::sdl::{SdlContext, SdlInstance};

pub struct EventSubsystem {
    native: SdlInstance,
}

impl EventSubsystem {
    pub fn poll_event(&mut self) -> Option<PlatformEventMessage> {
        self.native.poll_event()
    }

    /// Current keyboard or mouse state after polling. Gamepads use their own handles.
    pub fn is_pressed(&self, code: KeyCode) -> bool {
        self.native.is_pressed(code)
    }
}

impl EventSubsystem {
    pub(in crate::platform) fn new(context: Rc<SdlContext>) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !context.event_subsystem_taken.replace(true),
            "the platform thread already has an event subsystem"
        );
        Ok(Self {
            native: SdlInstance { context },
        })
    }
}

impl Drop for EventSubsystem {
    fn drop(&mut self) {
        self.native.context.event_subsystem_taken.set(false);
    }
}
