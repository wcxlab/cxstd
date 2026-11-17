//! Call `ecs.add_plugin(View3dPlugin::default())` to register
//! the View3d render system.
use cxstd::ecs::{Plugin, PluginContext};

#[derive(Default)]
pub(super) struct View3dPlugin;

impl Plugin for View3dPlugin {
    type LoadParams = ();

    fn load(&mut self, ctx: PluginContext, _deps: ()) -> anyhow::Result<()> {
        ctx.add_system("render", build_view3d_system);
        Ok(())
    }
}

pub(crate) fn build_view3d_system(_: cxstd::ecs::SystemContext, _deps: ()) {}
