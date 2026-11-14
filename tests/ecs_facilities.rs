mod support;

use cxstd::{Resource, Result, ecs::CommandQueue};
use std::{
    cell::Cell,
    future::pending,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    time::{Duration, Instant},
};

fn wait_until(mut ready: impl FnMut() -> Result<bool>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready()? {
        anyhow::ensure!(Instant::now() < deadline, "ECS operation timed out");
        std::thread::yield_now();
    }
    Ok(())
}

#[derive(Default)]
struct Trace(Vec<&'static str>);
impl Resource for Trace {}

#[test]
fn commands_wait_for_barriers_and_keep_new_work_for_the_next_batch() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let value = ecs.world_mut().spawn(0_u32);
    ecs.add_resource(Trace::default())?;
    let observed = Rc::new(Cell::new(0));
    let calls = observed.clone();
    ecs.add_system("update", move |commands: CommandQueue| {
        calls.set(calls.get() + 1);
        commands.queue_with(move |world, _, access| {
            *world.state_mut(value)? += 1;
            access.resources().get_mut::<Trace>()?.0.push("command");
            access.command_queue().queue_with(|_, _, access| {
                access
                    .resources()
                    .get_mut::<Trace>()?
                    .0
                    .push("next barrier");
                Ok(())
            });
            Ok(())
        });
    });
    ecs.run()?;
    assert!(ecs.resource::<Trace>()?.0.is_empty());
    assert_eq!(*value.read(ecs.world())?, 0);
    assert_eq!(ecs.apply_commands(&mut ())?, 1);
    ecs.add_apply_deferred("update");
    ecs.run()?;
    assert_eq!(observed.get(), 2);
    assert_eq!(*value.read(ecs.world())?, 2);
    assert_eq!(
        ecs.resource::<Trace>()?.0,
        ["command", "next barrier", "command"]
    );
    assert_eq!(ecs.command_queue().len(), 1);
    Ok(())
}

#[test]
fn failed_commands_preserve_old_work_before_new_and_stop_systems() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Trace::default())?;
    ecs.command_queue().queue_with(|_, _, access| {
        access.command_queue().queue_with(|_, _, access| {
            access.resources().get_mut::<Trace>()?.0.push("new");
            Ok(())
        });
        anyhow::bail!("command failure")
    });
    ecs.command_queue().queue_with(|_, _, access| {
        access.resources().get_mut::<Trace>()?.0.push("retained");
        Ok(())
    });
    ecs.add_apply_deferred("update")
        .add_system("update", || -> () { panic!("later system ran") });
    assert!(ecs.run().is_err());
    assert_eq!(ecs.apply_commands(&mut ())?, 2);
    assert_eq!(ecs.resource::<Trace>()?.0, ["retained", "new"]);
    Ok(())
}

#[test]
fn scoped_resources_restore_after_errors_and_panics_with_world_access() -> Result<()> {
    struct Local(Rc<Cell<usize>>);
    impl Resource for Local {}
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let count = Rc::new(Cell::new(0));
    ecs.add_resource(Local(count.clone()))?;
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = ecs.with_resource::<Local, ()>(|resource, ecs| {
                resource.0.set(1);
                ecs.world_mut()
                    .spawn(String::from("scoped resource can access entities"));
                assert!(ecs.resource::<Local>().is_err());
                assert!(ecs.add_resource(Local(count.clone())).is_err());
                panic!("expected unwind");
            });
        }))
        .is_err()
    );
    assert_eq!(ecs.resource::<Local>()?.0.get(), 1);
    assert_eq!(ecs.world().entities().count(), 1);
    assert!(
        ecs.with_resource::<Local, ()>(|resource, _| {
            resource.0.set(2);
            anyhow::bail!("expected resource error")
        })
        .is_err()
    );
    assert_eq!(ecs.resource::<Local>()?.0.get(), 2);
    Ok(())
}

#[test]
fn dropping_an_ecs_releases_its_resources_and_state() -> Result<()> {
    struct Local(Rc<()>);
    impl Resource for Local {}
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let state_lifetime = Rc::new(());
    let resource_lifetime = Rc::new(());
    ecs.add_resource(Local(resource_lifetime.clone()))?;
    let entity = ecs.world_mut().spawn(state_lifetime.clone());
    assert_eq!(Rc::strong_count(&state_lifetime), 2);
    assert_eq!(Rc::strong_count(&ecs.resource::<Local>()?.0), 2);
    drop(ecs);
    assert_eq!(Rc::strong_count(&state_lifetime), 1);
    assert_eq!(Rc::strong_count(&resource_lifetime), 1);
    let (_runtime2, other) = support::ecs::<()>()?;
    assert!(entity.read(other.world()).is_err());
    Ok(())
}

#[test]
fn targeted_events_are_typed_batched_cancelled_and_pruned_after_despawn() -> Result<()> {
    struct Edited(u32);
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let panel = ecs.world_mut().spawn(0_u32);
    let other = ecs.world_mut().spawn(0_u32);
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    let changed = ecs.observe(panel.entity(), move |_, _| {
        observed.set(observed.get() + 1);
        Ok(())
    });
    let events = ecs.event_dispatcher().clone();
    let edited = ecs.subscribe(panel.entity(), move |event: &Edited, world, _| {
        *world.state_mut(panel)? += event.0;
        events.notify(panel.entity());
        Ok(())
    });
    ecs.emit(other.entity(), Edited(100));
    ecs.emit(panel.entity(), "another type");
    ecs.emit(panel.entity(), Edited(3));
    assert_eq!(ecs.dispatch_events(&mut ()), 3);
    assert_eq!(*panel.read(ecs.world())?, 3);
    assert_eq!(count.get(), 0);
    assert_eq!(ecs.dispatch_events(&mut ()), 1);
    assert_eq!(count.get(), 1);
    drop(edited);
    ecs.emit(panel.entity(), Edited(10));
    ecs.dispatch_events(&mut ());
    assert_eq!(*panel.read(ecs.world())?, 3);
    ecs.observe(panel.entity(), |_, _| panic!("dead target ran"))
        .detach();
    ecs.notify(panel.entity());
    ecs.world_mut().despawn(panel.entity());
    assert_eq!(ecs.dispatch_events(&mut ()), 0);
    ecs.emit(panel.entity(), Edited(1));
    assert_eq!(ecs.dispatch_events(&mut ()), 0);
    drop(changed);
    Ok(())
}

#[test]
fn deleting_a_target_skips_later_callbacks_and_releases_captures() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let target = ecs.world_mut().spawn_empty();
    let lifetime = Rc::new(());
    let captured = lifetime.clone();
    let first = ecs.subscribe(target, move |_: &(), world, _| {
        assert!(world.despawn(target));
        Ok(())
    });
    ecs.subscribe(target, move |_: &(), _, _| {
        let _keep_capture = &captured;
        panic!("removed target callback ran");
    })
    .detach();
    assert_eq!(Rc::strong_count(&lifetime), 2);
    ecs.emit(target, ());
    assert_eq!(ecs.dispatch_events(&mut ()), 1);
    assert_eq!(Rc::strong_count(&lifetime), 1);
    drop(first);
    Ok(())
}

#[test]
fn async_completion_uses_owner_facilities_and_observers_wait_for_the_next_batch() -> Result<()> {
    struct Edited(u32);
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let value = ecs.world_mut().spawn(0_u32);
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    let events = ecs.event_dispatcher().clone();
    let edited = ecs.subscribe(value.entity(), move |event: &Edited, world, _| {
        *world.state_mut(value)? = event.0;
        events.notify(value.entity());
        Ok(())
    });
    let changed = ecs.observe(value.entity(), move |world, _| {
        assert_eq!(*value.read(world)?, 7);
        observed.set(observed.get() + 1);
        Ok(())
    });
    let task = ecs.spawn(async { 7 }, move |output, world, _, access| {
        assert_eq!(*value.read(world)?, 0);
        access
            .event_dispatcher()
            .emit(value.entity(), Edited(output));
        Ok(())
    })?;
    wait_until(|| {
        ecs.flush()?;
        Ok(*value.read(ecs.world())? == 7)
    })?;
    assert_eq!(count.get(), 0);
    drop(edited);
    ecs.emit(value.entity(), Edited(99));
    ecs.flush()?;
    assert_eq!(*value.read(ecs.world())?, 7);
    assert_eq!(count.get(), 1);
    drop((changed, task));
    Ok(())
}

#[test]
fn observer_and_completion_errors_return_at_command_barriers() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let target = ecs.world_mut().spawn_empty();
    let changed = ecs.observe(target, |_, _| anyhow::bail!("observer failure"));
    ecs.notify(target);
    assert!(format!("{:#}", ecs.flush().unwrap_err()).contains("observer failure"));
    let task = ecs.spawn(
        async { Err::<(), _>(anyhow::anyhow!("load failure")) },
        |result, _, _, _| result,
    )?;
    wait_until(|| match ecs.flush() {
        Ok(()) => Ok(false),
        Err(error) => {
            assert!(format!("{error:#}").contains("load failure"));
            Ok(true)
        }
    })?;
    drop((changed, task));
    Ok(())
}

#[test]
fn dropping_an_ecs_cancels_tasks_and_breaks_owned_observer_cycles() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let target = ecs.world_mut().spawn_empty();
    let lifetime = Rc::new(());
    let capture = lifetime.clone();
    let events = ecs.event_dispatcher().clone();
    ecs.observe(target, move |_, _| {
        let _keep = &capture;
        events.notify(target);
        Ok(())
    })
    .detach();
    let task = ecs.task_executor().spawn_local(pending::<()>())?;
    let command_lifetime = Rc::new(());
    let command_capture = command_lifetime.clone();
    let commands = ecs.command_queue();
    ecs.command_queue().queue(move |_, _| {
        let _keep = &command_capture;
        commands.queue(|_, _| Ok(()));
        Ok(())
    });
    assert_eq!(Rc::strong_count(&lifetime), 2);
    assert_eq!(Rc::strong_count(&command_lifetime), 2);
    drop(ecs);
    assert!(task.is_finished());
    assert_eq!(Rc::strong_count(&lifetime), 1);
    assert_eq!(Rc::strong_count(&command_lifetime), 1);
    Ok(())
}

#[test]
fn ecs_instances_isolate_resources_tasks_events_and_observers() -> Result<()> {
    let (_runtime1, mut first) = support::ecs::<()>()?;
    let (_runtime2, mut second) = support::ecs::<()>()?;
    first.add_resource(Trace(vec!["first"]))?;
    second.add_resource(Trace(vec!["second"]))?;
    let first_target = first.world_mut().spawn_empty();
    let second_target = second.world_mut().spawn_empty();
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    let changed = second.observe(second_target, move |_, _| {
        observed.set(observed.get() + 1);
        Ok(())
    });
    first.notify(second_target);
    second.notify(second_target);
    first.flush()?;
    assert_eq!(count.get(), 0);
    second.flush()?;
    assert_eq!(count.get(), 1);
    assert!(!first.world().contains(second_target));
    assert!(!second.world().contains(first_target));
    let first_task = first.task_executor().spawn_local(pending::<()>())?;
    let second_task = second.task_executor().spawn_local(pending::<()>())?;
    drop(first);
    assert!(first_task.is_finished());
    assert!(!second_task.is_finished());
    second.notify(second_target);
    second.flush()?;
    assert_eq!(count.get(), 2);
    assert_eq!(second.resource::<Trace>()?.0, ["second"]);
    drop(changed);
    Ok(())
}
