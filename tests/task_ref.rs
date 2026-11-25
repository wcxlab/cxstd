mod support;

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use cxstd::ecs::{Ecs, Plugin, PluginContext, ResMut, Resource, System, SystemContext, TaskRef};

struct Count(u32);
impl Resource for Count {}

fn wait_until(mut ready: impl FnMut() -> anyhow::Result<bool>) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready()? {
        anyhow::ensure!(Instant::now() < deadline, "task completion timed out");
        std::thread::yield_now();
    }
    Ok(())
}

#[test]
fn references_outlive_contexts_and_do_not_own_task_lifetimes() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Count(0))?;
    let owner = std::thread::current().id();
    let task: TaskRef = ecs
        .context()
        .add_task(async { 7 }, move |output, _, _, access| {
            assert_eq!(std::thread::current().id(), owner);
            access.resource_mut::<Count>()?.0 = output;
            Ok(())
        })?;
    let reference = task.clone();
    assert_eq!(task.id(), reference.id());
    drop(task);
    // Future completion and executor delivery cannot mutate ECS resources.
    let resource = ecs.resource::<Count>()?;
    wait_until(|| {
        ecs.task_executor().update();
        Ok(ecs.task_executor().pending_count() == 0)
    })?;
    assert_eq!(resource.0, 0);
    assert!(!reference.is_finished());
    let completion = reference.clone();
    drop(reference);
    drop(resource);
    ecs.flush()?;
    assert!(completion.is_finished());
    assert_eq!(ecs.resource::<Count>()?.0, 7);
    Ok(())
}

#[test]
fn task_references_finish_when_the_owning_ecs_releases_pending_work() -> anyhow::Result<()> {
    let (_runtime, ecs) = support::ecs::<()>()?;
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    let task = ecs.add_task(std::future::pending::<()>(), move |_, _, _, _| {
        let _capture = capture;
        Ok(())
    })?;
    assert!(!task.is_finished());
    drop(ecs);
    assert!(task.is_finished());
    assert!(weak.upgrade().is_none());
    Ok(())
}

#[test]
fn abort_suppresses_a_completion_already_waiting_at_the_barrier() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let called = Rc::new(Cell::new(false));
    let completed = called.clone();
    let task = ecs.add_task(async {}, move |_, _, _, _| {
        completed.set(true);
        Ok(())
    })?;
    wait_until(|| {
        ecs.task_executor().update();
        Ok(ecs.task_executor().pending_count() == 0)
    })?;
    assert!(!task.is_finished());
    task.clone().abort();
    assert!(task.is_finished());
    ecs.flush()?;
    assert!(!called.get());
    Ok(())
}

#[test]
fn completions_use_only_the_owning_ecs_and_report_errors() -> anyhow::Result<()> {
    let (_runtime, mut first) = support::ecs::<()>()?;
    let (_other_runtime, mut second) = support::ecs::<()>()?;
    first.add_resource(Count(1))?;
    second.add_resource(Count(2))?;
    let task = first.add_task(async {}, |_, _, _, access| {
        access.resource_mut::<Count>()?.0 += 10;
        anyhow::bail!("completion failure")
    })?;
    wait_until(|| {
        first.task_executor().update();
        Ok(first.task_executor().pending_count() == 0)
    })?;
    second.flush()?;
    assert_eq!(first.resource::<Count>()?.0, 1);
    assert_eq!(second.resource::<Count>()?.0, 2);
    assert!(
        first
            .flush()
            .unwrap_err()
            .to_string()
            .contains("completion failure")
    );
    assert_eq!(first.resource::<Count>()?.0, 11);
    drop(task);
    first.task_executor().close();
    assert!(
        first
            .context()
            .add_task(async {}, |_, _, _, _| Ok(()))
            .is_err()
    );
    Ok(())
}

struct AddTask;
impl System for AddTask {
    type Deps = ResMut<'static, Count>;

    fn run(&mut self, ctx: SystemContext, mut deps: ResMut<Count>) -> anyhow::Result<()> {
        deps.0 += 1;
        ctx.add_task(async { 10 }, |output, _, _, access| {
            access.resource_mut::<Count>()?.0 += output;
            Ok(())
        })?;
        Ok(())
    }
}

struct Tasks;
impl Plugin for Tasks {
    type Deps = ();

    fn init(&mut self, ctx: PluginContext, _deps: ()) -> anyhow::Result<()> {
        ctx.add_stage("update");
        ctx.add_system("update", AddTask)?;
        ctx.add_task(async { 2 }, |output, _, _, access| {
            access.resource_mut::<Count>()?.0 += output;
            Ok(())
        })?;
        Ok(())
    }
}

#[test]
fn plugin_and_system_contexts_register_owned_tasks() -> anyhow::Result<()> {
    let (_runtime, mut ecs): (_, Ecs) = support::ecs()?;
    ecs.add_resource(Count(0))?;
    ecs.add_plugin(Tasks)?;
    ecs.run_stage("update")?;
    wait_until(|| {
        ecs.flush()?;
        Ok(ecs.resource::<Count>()?.0 == 13)
    })?;
    Ok(())
}
