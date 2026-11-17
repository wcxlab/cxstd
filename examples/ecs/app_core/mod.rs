//! Business ECS through `AppCore::new()`, `update()` and the `status()` snapshot.
//! Drop unloads the status plugin before the ECS is destroyed.
//! Status clones retain timing snapshots and share the live asynchronous counter.

mod status;
use cxstd::ecs::{Ecs, PluginRef};

pub(super) use status::Status;
use status::StatusPlugin;

pub(super) struct AppCore {
    ecs: Ecs,
    status_plugin: PluginRef<StatusPlugin>,
}

impl AppCore {
    pub(super) fn new() -> anyhow::Result<Self> {
        let mut ecs = Ecs::new()?;
        let status_plugin = ecs.add_plugin(StatusPlugin::default())?;
        Ok(Self { ecs, status_plugin })
    }

    pub(super) fn status(&self) -> anyhow::Result<Status> {
        Ok(self.ecs.resource::<Status>()?.clone())
    }

    pub(super) fn update(&mut self) -> anyhow::Result<()> {
        self.ecs.run_stage("pre_update")?;
        let result = self.ecs.run_stage("update");
        self.ecs.run_stage("post_update")?;
        result
    }
}

impl Drop for AppCore {
    fn drop(&mut self) {
        if let Err(error) = self.ecs.remove_plugin(self.status_plugin) {
            eprintln!("Failed to unload StatusPlugin: {error:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_view::AppView;
    use cxstd::ecs::{Entity, ResMut, Resource, World};

    #[derive(Default)]
    struct Frames(usize);
    impl Resource for Frames {}

    #[tokio::test]
    async fn view_updates_do_not_mutate_business_state() -> anyhow::Result<()> {
        let mut core = AppCore::new()?;
        core.ecs.resources_mut().add(Frames(99))?;
        core.ecs.add_system(
            "update",
            |_: cxstd::ecs::SystemContext<'_, _>, deps: ResMut<Frames>| {
                let mut frames = deps;
                frames.0 += 1;
            },
        );
        let business = core.ecs.world_mut().add_entity(Entity::with_state(10_u32));
        let business = core.ecs.world_mut().typed::<u32>(business)?;
        core.ecs
            .add_system("update", move |world: &mut World| -> anyhow::Result<()> {
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
