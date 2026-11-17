mod support;
use cxstd::Resource;
use cxstd::ecs::{CommandQueue, Entity, ResMut, World};

#[derive(Default)]
struct Trace(Vec<usize>);
impl Resource for Trace {}

#[test]
fn selected_stages_follow_caller_order_and_preserve_system_state() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Trace::default())?;
    ecs.add_schedule("second").add_schedule("first");
    ecs.add_system(
        "first",
        support::system(|_: cxstd::ecs::SystemContext<'_, _>, deps: ResMut<Trace>| {
            let mut trace = deps;
            trace.0.push(1)
        }),
    );
    let mut calls = 0;
    ecs.add_system(
        "second",
        support::system(
            move |_: cxstd::ecs::SystemContext<'_, _>, deps: ResMut<Trace>| {
                let mut trace = deps;
                calls += 1;
                trace.0.push(calls * 10);
            },
        ),
    );
    ecs.add_schedule("second");
    ecs.add_system(
        "third",
        support::system(|_: cxstd::ecs::SystemContext<'_, _>, deps: ResMut<Trace>| {
            let mut trace = deps;
            trace.0.push(3)
        }),
    );
    ecs.run_stage("first")?;
    assert_eq!(ecs.resource::<Trace>()?.0, [1]);
    ecs.run_stage("second")?;
    ecs.run_stage("third")?;
    ecs.run_stage("second")?;
    assert_eq!(ecs.resource::<Trace>()?.0, [1, 10, 3, 20]);
    Ok(())
}

#[test]
fn stage_errors_preserve_changes_and_stop_later_stages() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Trace::default())?;
    ecs.add_system(
        "first",
        support::system(|_: cxstd::ecs::SystemContext<'_, _>, deps: ResMut<Trace>| {
            let mut trace = deps;
            trace.0.push(1)
        }),
    );
    ecs.add_system(
        "second",
        support::system(
            |_: cxstd::ecs::SystemContext<'_, _>, deps: ResMut<Trace>| -> anyhow::Result<()> {
                let mut trace = deps;
                trace.0.push(2);
                anyhow::bail!("expected failure")
            },
        ),
    );
    ecs.add_system(
        "third",
        support::system(|_: cxstd::ecs::SystemContext<'_, _>, _deps: ()| -> () {
            panic!("later stage ran");
        }),
    );
    let error = format!("{:#}", ecs.run().unwrap_err());
    assert!(error.contains("second"));
    assert!(error.contains("expected failure"));
    assert_eq!(ecs.resource::<Trace>()?.0, [1, 2]);
    Ok(())
}

#[test]
fn command_barriers_apply_to_the_owning_ecs_world() -> anyhow::Result<()> {
    let (_runtime1, mut first) = support::ecs::<()>()?;
    let (_runtime2, mut second) = support::ecs::<()>()?;
    let other = second.world_mut().add_entity(Entity::with_state(7_u32));
    let other = second.world_mut().typed::<u32>(other)?;
    first.add_system(
        "first",
        support::system(|_: cxstd::ecs::SystemContext<'_, _>, deps: CommandQueue| {
            let commands = deps;
            commands.queue(|world, _| {
                world.add_entity(Entity::with_state(9_u32));
                Ok(())
            });
        }),
    );
    first.add_apply_deferred("first");
    first.add_system(
        "second",
        support::system(|_: cxstd::ecs::SystemContext<'_, _>, deps: &World| {
            let world = deps;
            assert_eq!(world.entities().count(), 1);
        }),
    );
    first.run()?;
    assert!(!first.world().contains(other.entity()));
    assert_eq!(*other.read(second.world())?, 7);
    assert_eq!(first.command_queue().len(), 0);
    Ok(())
}

#[test]
fn stage_update_advances_facilities_and_runs_only_the_selected_stage_once() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Trace::default())?;
    ecs.command_queue().queue_with(|_, _, access| {
        access.resources().get_mut::<Trace>()?.0.push(0);
        Ok(())
    });
    ecs.add_system(
        "first",
        support::system(
            |_: cxstd::ecs::SystemContext<'_, _>, deps: (ResMut<Trace>, CommandQueue)| {
                let (mut trace, commands) = deps;
                trace.0.push(1);
                commands.queue_with(|_, _, access| {
                    access.resources().get_mut::<Trace>()?.0.push(2);
                    Ok(())
                });
            },
        ),
    );
    ecs.add_system(
        "second",
        support::system(|_: cxstd::ecs::SystemContext<'_, _>, _deps: ()| -> () {
            panic!("unselected stage ran");
        }),
    );
    ecs.run_stage("first")?;
    assert_eq!(ecs.resource::<Trace>()?.0, [0, 1]);
    assert_eq!(ecs.command_queue().len(), 1);
    ecs.run_stage("first")?;
    assert_eq!(ecs.resource::<Trace>()?.0, [0, 1, 2, 1]);
    Ok(())
}

#[test]
fn unknown_stage_fails_before_applying_pending_commands() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Trace::default())?;
    ecs.add_schedule("first");
    ecs.command_queue().queue_with(|_, _, ctx| {
        ctx.resource_mut::<Trace>()?.0.push(1);
        Ok(())
    });
    let error = format!("{:#}", ecs.run_stage("third").unwrap_err());
    assert!(error.contains("third"));
    assert!(error.contains("not registered"));
    assert!(ecs.resource::<Trace>()?.0.is_empty());
    assert_eq!(ecs.command_queue().len(), 1);
    ecs.run_stage("first")?;
    assert_eq!(ecs.resource::<Trace>()?.0, [1]);
    Ok(())
}
