//! Create a `View3dPlugin::default()` instance and call `plugin.load(ecs)` to register
//! the View3d render system.

use anyhow::Result;
use cxstd::ecs::{Ecs, Plugin};

#[derive(Default)]
pub(super) struct View3dPlugin;

impl Plugin for View3dPlugin {
    fn load(&mut self, ecs: &mut Ecs) -> Result<()> {
        ecs.add_system("render", build_view3d_system);
        Ok(())
    }
}

pub(crate) fn build_view3d_system() {}
