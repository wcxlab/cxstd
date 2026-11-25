//! Independent view ECS through `AppView::new()`, `update(&core)` and `render()`.
//! Drop removes plugins in reverse registration order before the ECS is destroyed.

mod render;
pub(super) mod view2d;
pub(super) mod view3d;
use cxstd::ecs::{Ecs, PluginRef};

use crate::app_core::AppCore;
use render::RenderPlugin;
use view2d::View2dPlugin;
use view3d::View3dPlugin;

pub(super) struct AppView {
    ecs: Ecs,
    view2d: PluginRef<View2dPlugin>,
    view3d: PluginRef<View3dPlugin>,
    render: PluginRef<RenderPlugin>,
}

impl AppView {
    pub(super) fn new() -> anyhow::Result<Self> {
        let mut ecs = Ecs::new()?;
        ecs.add_stage("update");
        ecs.add_stage("render");
        let view2d = ecs.add_plugin(View2dPlugin::default())?;
        let view3d = ecs.add_plugin(View3dPlugin::default())?;
        let render = ecs.add_plugin(RenderPlugin::default())?;
        Ok(Self {
            ecs,
            view2d,
            view3d,
            render,
        })
    }

    pub(super) fn update(&mut self, core: &AppCore) -> anyhow::Result<()> {
        self.ecs.add_resource(core.status()?)?;
        self.ecs.run_stage("update")
    }

    pub(super) fn render(&mut self) -> anyhow::Result<()> {
        self.ecs.run_stage("render")
    }
}

impl Drop for AppView {
    fn drop(&mut self) {
        self.ecs.remove_plugin(self.render).unwrap();
        self.ecs.remove_plugin(self.view3d).unwrap();
        self.ecs.remove_plugin(self.view2d).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_core::Status;

    #[tokio::test]
    async fn status_is_copied_from_core_before_view_updates() -> anyhow::Result<()> {
        let mut core = AppCore::new()?;
        let mut view = AppView::new()?;
        assert!(view.ecs.resource::<Status>().is_err());
        core.update()?;
        let core_status = core.status()?;
        view.update(&core)?;
        let view_status = view.ecs.resource::<Status>()?;
        assert_eq!(view_status.current_time, core_status.current_time);
        assert_eq!(view_status.update_duration, core_status.update_duration);
        assert_eq!(core.status()?.current_time, core_status.current_time);
        assert_eq!(core.status()?.update_duration, core_status.update_duration);
        Ok(())
    }
}
