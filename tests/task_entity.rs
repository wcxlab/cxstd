mod support;

use std::{cell::Cell, future::pending, rc::Rc};

use anyhow::Result;
use cxstd::ecs::{Component, TaskEntity};

struct Label;
impl Component for Label {}

#[test]
fn task_entities_start_on_flush_and_complete_only_once_on_the_owner_thread() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let owner = std::thread::current().id();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let mut task = TaskEntity::new(async move {
        assert_eq!(std::thread::current().id(), owner);
        observed.set(observed.get() + 1);
        42_u32
    });
    task.add_component(Label);
    assert_eq!(ecs.world().entities().count(), 0);
    let id = ecs.world_mut().add_entity(task);
    assert_eq!(calls.get(), 0);
    assert_eq!(ecs.world().query::<Label>().count(), 1);
    ecs.flush()?;
    assert_eq!(calls.get(), 1);
    ecs.flush()?;
    assert_eq!(calls.get(), 1);
    assert!(ecs.world().contains(id));
    Ok(())
}

struct Lifetime(Rc<Cell<bool>>);
impl Drop for Lifetime {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

#[test]
fn despawning_or_dropping_the_world_cancels_owned_tasks() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let released = Rc::new(Cell::new(false));
    let lifetime = Lifetime(released.clone());
    let id = ecs.world_mut().add_entity(TaskEntity::new(async move {
        let _lifetime = lifetime;
        pending::<()>().await;
    }));
    ecs.flush()?;
    assert!(!released.get());
    ecs.world_mut().despawn(id);
    ecs.flush()?;
    assert!(released.get());

    released.set(false);
    let lifetime = Lifetime(released.clone());
    ecs.world_mut().add_entity(TaskEntity::new(async move {
        let _lifetime = lifetime;
        pending::<()>().await;
    }));
    ecs.flush()?;
    drop(ecs);
    assert!(released.get());
    Ok(())
}

#[test]
fn despawning_before_first_flush_never_runs_the_future() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let id = ecs.world_mut().add_entity(TaskEntity::new(async move {
        observed.set(1);
    }));
    ecs.world_mut().despawn(id);
    ecs.flush()?;
    assert_eq!(calls.get(), 0);
    Ok(())
}

#[test]
fn ecs_preserves_a_typed_task_error_when_propagating_it_through_anyhow() -> Result<()> {
    use anyhow::Context as _;
    use cxstd::ecs::AsyncClosed;

    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.task_executor().close();
    ecs.world_mut().add_entity(TaskEntity::new(async {}));
    let error = ecs.flush().context("failed to advance ECS").unwrap_err();
    assert!(error.downcast_ref::<AsyncClosed>().is_some());
    assert_eq!(error.to_string(), "failed to advance ECS");
    assert_eq!(
        error.downcast::<AsyncClosed>()?.to_string(),
        "asynchronous operation is closed"
    );
    Ok(())
}
