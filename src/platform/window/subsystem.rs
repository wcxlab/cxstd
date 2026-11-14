//! Window construction through the owning platform's shared context.

use std::rc::Rc;

use crate::platform::backend::sdl::SdlContext;
use crate::window::WgpuWindowBuilder;

pub struct WindowSubsystem {
    pub(in crate::platform) context: Rc<SdlContext>,
}

impl WindowSubsystem {
    pub fn wgpu_window(&self) -> WgpuWindowBuilder {
        WgpuWindowBuilder::with_context(self.context.clone())
    }
}
