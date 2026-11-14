//! Business ECS through `AppCore::new()`, `update()` and the `status()` snapshot.
//! Drop unloads the status plugin before the ECS is destroyed.
//! Status clones retain timing snapshots and share the live asynchronous counter.

mod status;

use cxstd::Result;
use cxstd::ecs::{Ecs, Plugin};

pub(super) use status::Status;
use status::StatusPlugin;

pub(super) struct AppCore {
    ecs: Ecs,
    status_plugin: StatusPlugin,
}

impl AppCore {
    pub(super) fn new() -> Result<Self> {
        let mut core = Self {
            ecs: Ecs::new()?,
            status_plugin: StatusPlugin::default(),
        };
        core.status_plugin.load(&mut core.ecs)?;
        Ok(core)
    }

    pub(super) fn status(&self) -> Result<Status> {
        Ok(self.ecs.resource::<Status>()?.clone())
    }

    pub(super) fn update(&mut self) -> Result<()> {
        self.ecs.run_schedule("pre_update")?;
        let result = self.ecs.run_schedule("update");
        self.ecs.run_schedule("post_update")?;
        result
    }
}

impl Drop for AppCore {
    fn drop(&mut self) {
        if let Err(error) = self.status_plugin.unload(&mut self.ecs) {
            eprintln!("Failed to unload StatusPlugin: {error:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_view::AppView;
    use cxstd::ecs::{ResMut, Resource, World};

    #[derive(Default)]
    struct Frames(usize);
    impl Resource for Frames {}

    #[tokio::test]
    async fn view_updates_do_not_mutate_business_state() -> Result<()> {
        let mut core = AppCore::new()?;
        core.ecs.resources_mut().add(Frames(99))?;
        core.ecs.add_system("update", |mut frames: ResMut<Frames>| {
            frames.0 += 1;
        });
        let business = core.ecs.world_mut().spawn(10_u32);
        core.ecs
            .add_system("update", move |world: &mut World| -> Result<()> {
                *world.state_mut(business)? += 1;
                Ok(())
            });
        let mut view = AppView::new()?;
        core.update()?;
        view.update(&core)?;
        assert_eq!(*business.read(core.ecs.world())?, 11);
        assert_eq!(core.ecs.resources().get::<Frames>()?.0, 100);
        Ok(())
    }
}
