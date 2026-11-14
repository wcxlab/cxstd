//! `RenderPlugin::default()` supplies the shared PaintList and registers terminal output.
//! Load it after view plugins so `render_system` runs after their paint systems.
//! The first update adds a task entity owning the counter's asynchronous loop.

use anyhow::Result;
use cxstd::ecs::{Component, Ecs, Plugin, Res, Resource, TaskEntity, World};

use crate::app_core::Status;

#[derive(Default)]
pub(super) struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn load(&mut self, ecs: &mut Ecs) -> Result<()> {
        ecs.add_resource(PaintList::default())?;
        ecs.add_system("update", update_counter_system);
        ecs.add_system("render", render_system);
        Ok(())
    }

    fn unload(&mut self, ecs: &mut Ecs) -> Result<()> {
        let tasks: Vec<_> = ecs
            .world()
            .query::<CounterLoop>()
            .map(|(id, _)| id)
            .collect();
        for task in tasks {
            ecs.world_mut().despawn(task);
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct PaintList(pub(super) Vec<String>);
impl Resource for PaintList {}

struct CounterLoop;
impl Component for CounterLoop {}

fn update_counter_system(world: &mut World, status: Res<Status>) {
    if world.query::<CounterLoop>().next().is_none() {
        let status = status.clone();
        let mut task = TaskEntity::new(async move {
            loop {
                status.increment_counter().await;
            }
        });
        task.add_component(CounterLoop);
        world.add_entity(task);
    }
}

fn render_system(list: Res<PaintList>) {
    println!("View tree:");
    for line in &list.0 {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app_core::AppCore, app_view::AppView};
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn counter_requests_wait_for_completion_and_cancel_on_unload() -> Result<()> {
        let core = AppCore::new()?;
        let mut view = AppView::new()?;
        let started = Instant::now();
        view.update(&core)?;
        view.update(&core)?;
        assert_eq!(view.ecs.world().query::<CounterLoop>().count(), 1);
        assert_eq!(core.status()?.counter(), 0);

        tokio::time::timeout(Duration::from_secs(2), async {
            while core.status()?.counter() < 1 {
                view.ecs.flush()?;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        assert!(started.elapsed() >= Duration::from_millis(100));
        assert_eq!(core.status()?.counter(), 1);

        // Deliver the completion and continue the chain without running an update system.
        tokio::time::timeout(Duration::from_secs(2), async {
            while core.status()?.counter() < 2 {
                view.ecs.flush()?;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        assert!(started.elapsed() >= Duration::from_millis(200));
        view.update(&core)?;
        view.render()?;
        let list = view.ecs.resource::<PaintList>()?;
        assert_eq!(list.0[2], list.0[5]);
        assert!(list.0[2].ends_with("counter=2"));
        drop(list);

        drop(view);
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(core.status()?.counter(), 2);
        Ok(())
    }
}
