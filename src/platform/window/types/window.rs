//! Window identity and native handles.

use std::{fmt, rc::Rc};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// A raw handle that keeps its native window alive while in use.
pub struct WindowHandle {
    raw: RawWindowHandle,
    owner: Rc<dyn HasWindowHandle>,
}

impl WindowHandle {
    pub(in crate::platform) fn new(raw: RawWindowHandle, owner: Rc<dyn HasWindowHandle>) -> Self {
        Self { raw, owner }
    }

    pub fn as_raw(&self) -> RawWindowHandle {
        self.raw
    }
}

impl HasWindowHandle for WindowHandle {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        self.owner.window_handle()
    }
}

impl fmt::Debug for WindowHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("WindowHandle").field(&self.raw).finish()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowId(pub u32);
