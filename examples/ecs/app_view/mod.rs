//! Independent view ECS through `AppView::new()`, `update(&core)` and `render()`.
//! Drop unloads plugins in reverse load order before the ECS is destroyed.

mod render;
pub(super) mod view2d;
pub(super) mod view3d;

use anyhow::Result;
use cxstd::ecs::{Ecs, Plugin};

use crate::app_core::AppCore;
use render::RenderPlugin;
use view2d::View2dPlugin;
use view3d::View3dPlugin;

pub(super) struct AppView {
    ecs: Ecs,
    view2d: View2dPlugin,
    view3d: View3dPlugin,
    render: RenderPlugin,
}

impl AppView {
    pub(super) fn new() -> Result<Self> {
        let mut view = Self {
            ecs: Ecs::new()?,
            view2d: View2dPlugin::default(),
            view3d: View3dPlugin::default(),
            render: RenderPlugin::default(),
        };
        view.view2d.load(&mut view.ecs)?;
        view.view3d.load(&mut view.ecs)?;
        view.render.load(&mut view.ecs)?;
        Ok(view)
    }

    pub(super) fn update(&mut self, core: &AppCore) -> Result<()> {
        self.ecs.add_resource(core.status()?)?;
        self.ecs.run_schedule("update")
    }

    pub(super) fn render(&mut self) -> Result<()> {
        self.ecs.run_schedule("render")
    }
}

impl Drop for AppView {
    fn drop(&mut self) {
        if let Err(error) = self.render.unload(&mut self.ecs) {
            eprintln!("Failed to unload RenderPlugin: {error:#}");
        }
        if let Err(error) = self.view3d.unload(&mut self.ecs) {
            eprintln!("Failed to unload View3dPlugin: {error:#}");
        }
        if let Err(error) = self.view2d.unload(&mut self.ecs) {
            eprintln!("Failed to unload View2dPlugin: {error:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_core::Status;

    #[tokio::test]
    async fn status_is_copied_from_core_before_view_updates() -> Result<()> {
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
