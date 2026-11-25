//! Event polling for the active native context.

use crate::platform::PlatformEventMessage;
use crate::platform::backend::sdl::SdlInstance;

pub(in crate::platform) struct EventService {
    native: SdlInstance,
}

impl EventService {
    pub(in crate::platform) fn new(native: SdlInstance) -> Self {
        Self { native }
    }

    pub(in crate::platform) fn current() -> Option<Self> {
        Some(Self::new(SdlInstance::current()?))
    }

    pub(in crate::platform) fn poll_event(&self) -> Option<PlatformEventMessage> {
        self.native.poll_event()
    }
}
