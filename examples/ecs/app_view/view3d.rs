//! Call `ecs.add_plugin(View3dPlugin::default())` to register
//! the View3d render system.
use cxstd::ecs::{Plugin, PluginContext, System, SystemContext};

#[derive(Default)]
pub(super) struct View3dPlugin;

impl Plugin for View3dPlugin {
    type Deps = ();

    fn init(&mut self, ctx: PluginContext, _deps: ()) -> anyhow::Result<()> {
        ctx.add_system("render", BuildView3dSystem)?;
        Ok(())
    }
}

pub(crate) struct BuildView3dSystem;

impl System for BuildView3dSystem {
    type Deps = ();

    fn run(&mut self, _ctx: SystemContext, _deps: ()) -> anyhow::Result<()> {
        Ok(())
    }
}
