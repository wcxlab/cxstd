mod support;

use cxstd::{
    Resource, Resources, Result,
    ecs::{
        CommandQueue, Component, EcsContext, EntityId, EventDispatcher, IntoSystem, Query, Res,
        ResMut, System, SystemAccess, SystemInput, SystemParam, TaskExecutor, World,
    },
};
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

struct Position(f32);
impl Component for Position {}
struct Velocity(f32);
impl Component for Velocity {}
struct Step(f32);
impl Resource for Step {}
#[derive(Default)]
struct Stats {
    frames: usize,
    entities: Vec<EntityId>,
}
impl Resource for Stats {}

fn integrate(
    mut objects: Query<(EntityId, &mut Position, &Velocity)>,
    step: Res<Step>,
    mut stats: ResMut<Stats>,
) {
    stats.frames += 1;
    for (entity, mut position, velocity) in objects.iter_mut() {
        position.0 += velocity.0 * step.0;
        stats.entities.push(entity);
    }
}

#[test]
fn functions_infer_parameters_and_queries_select_matching_entities() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let moving = ecs.world_mut().spawn_empty();
    ecs.world_mut().add_component(moving, Position(1.0))?;
    ecs.world_mut().add_component(moving, Velocity(2.0))?;
    let stationary = ecs.world_mut().spawn_empty();
    ecs.world_mut().add_component(stationary, Position(100.0))?;
    ecs.world_mut().spawn_empty();
    ecs.add_resource(Step(0.5))?;
    ecs.add_resource(Stats::default())?;
    ecs.add_system("update", integrate);
    for expected in [2.0, 3.0] {
        ecs.run()?;
        assert_eq!(ecs.world().component::<Position>(moving)?.0, expected);
    }
    assert_eq!(ecs.world().component::<Position>(stationary)?.0, 100.0);
    assert_eq!(ecs.resource::<Stats>()?.entities, [moving, moving]);
    let read = (move |all: Query<EntityId>, objects: Query<(EntityId, &Position)>| -> Result<()> {
        assert_eq!(all.iter().count(), 3);
        assert_eq!(objects.iter().count(), 2);
        assert_eq!(objects.get(moving)?.1.0, 4.0);
        assert!(objects.get(all.iter().last().unwrap()).is_err());
        Ok(())
    })
    .into_system();
    ecs.add_system("read_back", read);
    ecs.run()
}

#[test]
fn disjoint_mutable_queries_and_resources_work_in_any_parameter_order() -> Result<()> {
    struct Shared(u32);
    impl Component for Shared {}
    impl Resource for Shared {}
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let entity = ecs.world_mut().spawn_empty();
    ecs.world_mut().add_component(entity, Position(0.0))?;
    ecs.world_mut().add_component(entity, Velocity(1.0))?;
    ecs.world_mut().add_component(entity, Shared(1))?;
    ecs.add_resource(Shared(10))?;
    ecs.add_resource(Stats::default())?;
    ecs.add_system(
        "update",
        move |mut stats: ResMut<Stats>,
              mut positions: Query<&mut Position>,
              mut shared: ResMut<Shared>,
              mut velocities: Query<&mut Velocity>,
              components: Query<&Shared>|
              -> Result<()> {
            stats.frames += 1;
            positions.get_mut(entity)?.0 += 2.0;
            velocities.get_mut(entity)?.0 += 3.0;
            shared.0 += components.get(entity)?.0;
            Ok(())
        },
    );
    ecs.run()?;
    assert_eq!(ecs.world().component::<Position>(entity)?.0, 2.0);
    assert_eq!(ecs.world().component::<Velocity>(entity)?.0, 4.0);
    assert_eq!(ecs.resource::<Shared>()?.0, 11);
    assert_eq!(ecs.resource::<Stats>()?.frames, 1);
    Ok(())
}

#[test]
fn duplicate_reads_are_allowed_for_queries_and_resources() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let entity = ecs.world_mut().spawn_empty();
    ecs.world_mut().add_component(entity, Position(5.0))?;
    ecs.add_resource(Step(2.0))?;
    ecs.add_system(
        "update",
        move |a: Res<Step>,
              b: Res<Step>,
              first: Query<&Position>,
              second: Query<(&Position, &Position)>,
              resources: &Resources| {
            assert_eq!(a.0, b.0);
            assert_eq!(resources.get::<Step>().unwrap().0, 2.0);
            let (left, right) = second.get(entity).unwrap();
            assert_eq!(first.get(entity).unwrap().0, left.0);
            assert_eq!(left.0, right.0);
        },
    );
    ecs.run()
}

fn rejects<M>(system: impl IntoSystem<(), M>) -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_system("update", system);
    let error = ecs.run().unwrap_err();
    assert!(format!("{error:#}").contains("conflict"), "{error:#}");
    Ok(())
}

#[test]
fn conflicting_parameters_fail_before_the_body_even_in_empty_world() -> Result<()> {
    rejects(|_: Query<&mut Position>, _: Query<&Position>| -> () { panic!("query ran") })?;
    rejects(|_: Query<&Position>, _: Query<&mut Position>| -> () { panic!("query ran") })?;
    rejects(|_: Query<(&mut Position, &mut Position)>| -> () { panic!("query ran") })?;
    rejects(|_: ResMut<Step>, _: Res<Step>| -> () { panic!("resource ran") })?;
    rejects(|_: Res<Step>, _: ResMut<Step>| -> () { panic!("resource ran") })?;
    rejects(|_: ResMut<Step>, _: ResMut<Step>| -> () { panic!("resource ran") })?;
    rejects(|_: &World, _: Query<&mut Position>| -> () { panic!("owner ran") })?;
    rejects(|_: Query<&mut Position>, _: &World| -> () { panic!("owner ran") })?;
    rejects(|_: &Resources, _: ResMut<Step>| -> () { panic!("resource owner ran") })?;
    rejects(|_: ResMut<Step>, _: &Resources| -> () { panic!("resource owner ran") })?;
    Ok(())
}

#[test]
fn missing_resources_release_previously_fetched_guards() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Stats::default())?;
    ecs.add_system("update", |mut stats: ResMut<Stats>, _: Res<Step>| {
        stats.frames += 1;
    });
    assert!(format!("{:#}", ecs.run().unwrap_err()).contains("Step"));
    assert_eq!(ecs.resource::<Stats>()?.frames, 0);
    ecs.add_resource(Step(1.0))?;
    ecs.run()?;
    assert_eq!(ecs.resource::<Stats>()?.frames, 1);
    Ok(())
}

#[test]
fn error_and_panic_release_borrows_and_stop_later_systems() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let entity = ecs.world_mut().spawn_empty();
    ecs.world_mut().add_component(entity, Position(0.0))?;
    ecs.add_resource(Stats::default())?;
    ecs.add_system(
        "update",
        |mut stats: ResMut<Stats>, mut objects: Query<&mut Position>| -> Result<()> {
            stats.frames += 1;
            objects.iter_mut().next().unwrap().0 += 1.0;
            anyhow::bail!("system failure")
        },
    )
    .add_system("update", || -> () { panic!("later system ran") });
    assert!(format!("{:#}", ecs.run().unwrap_err()).contains("system failure"));
    assert_eq!(ecs.resource::<Stats>()?.frames, 1);
    assert_eq!(ecs.world().component::<Position>(entity)?.0, 1.0);

    let (_runtime2, mut panic_ecs) = support::ecs::<()>()?;
    let entity = panic_ecs.world_mut().spawn_empty();
    panic_ecs.world_mut().add_component(entity, Position(0.0))?;
    panic_ecs.add_resource(Stats::default())?;
    panic_ecs.add_system(
        "update",
        |mut stats: ResMut<Stats>, mut objects: Query<&mut Position>| -> () {
            stats.frames += 1;
            let mut position = objects.iter_mut().next().unwrap();
            position.0 += 1.0;
            panic!("expected unwind");
        },
    );
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = panic_ecs.run();
        }))
        .is_err()
    );
    assert_eq!(panic_ecs.resource::<Stats>()?.frames, 1);
    assert_eq!(panic_ecs.world().component::<Position>(entity)?.0, 1.0);
    Ok(())
}

#[test]
fn command_event_and_task_parameters_use_the_executing_ecs() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let target = ecs.world_mut().spawn(0_u32);
    ecs.add_resource(Stats::default())?;
    let subscription = ecs.subscribe(target.entity(), move |value: &u32, world, _| {
        *world.state_mut(target)? = *value;
        Ok(())
    });
    let completed = Rc::new(Cell::new(false));
    let completion = completed.clone();
    ecs.add_system(
        "update",
        move |commands: CommandQueue,
              events: &EventDispatcher<EntityId, World>,
              tasks: &TaskExecutor,
              mut stats: ResMut<Stats>|
              -> Result<()> {
            events.emit(target.entity(), 7_u32);
            stats.frames += 1;
            commands.queue_with(|world, _, access| {
                assert_eq!(world.entities().count(), 1);
                access.resources().get_mut::<Stats>()?.frames += 10;
                Ok(())
            });
            let completion = completion.clone();
            tasks
                .spawn_local(async move {
                    completion.set(true);
                })?
                .detach();
            Ok(())
        },
    );
    ecs.run()?;
    assert_eq!(*target.read(ecs.world())?, 0);
    assert_eq!(ecs.resource::<Stats>()?.frames, 1);
    assert!(!completed.get());
    ecs.flush()?;
    assert_eq!(*target.read(ecs.world())?, 7);
    assert_eq!(ecs.resource::<Stats>()?.frames, 11);
    assert!(completed.get());
    drop(subscription);
    Ok(())
}

#[test]
fn closures_exclusive_functions_and_custom_systems_coexist() -> Result<()> {
    #[derive(Default)]
    struct Host {
        calls: usize,
    }
    impl SystemParam<Host> for &Host {
        type Item<'w> = &'w Host;
        fn access(_: &mut SystemAccess) -> Result<()> {
            Ok(())
        }
        fn fetch(input: SystemInput<'_, Host>) -> Result<Self::Item<'_>> {
            Ok(input.context)
        }
    }
    struct Custom;
    impl System<Host> for Custom {
        fn run(&mut self, _: &mut World, _: &mut Host, access: EcsContext<'_, Host>) -> Result<()> {
            access.resources().get_mut::<Stats>()?.frames += 100;
            Ok(())
        }
    }
    let (_runtime, mut ecs) = support::ecs::<Host>()?;
    let mut host = Host::default();
    ecs.add_resource(Stats::default())?;
    let observed = Rc::new(Cell::new(0));
    let calls = observed.clone();
    let mut local_calls = 0;
    ecs.add_system("update", || {})
        .add_system("update", move |world: &World, _: Res<Stats>, _: &Host| {
            local_calls += 1;
            calls.set(local_calls);
            assert_eq!(world.entities().count(), local_calls - 1);
        })
        .add_system("update", |world: &mut World| {
            world.spawn_empty();
        })
        .add_system("update", |host: &mut Host| {
            host.calls += 1;
        })
        .add_system("update", Custom)
        .add_system("update", |host: &mut Host, world: &mut World| {
            host.calls += world.entities().count();
        });
    ecs.run_schedule_with("update", &mut host)?;
    ecs.run_schedule_with("update", &mut host)?;
    assert_eq!(observed.get(), 2);
    assert_eq!(host.calls, 5);
    assert_eq!(ecs.resource::<Stats>()?.frames, 200);
    Ok(())
}

#[test]
fn custom_parameters_compose_existing_fetches_and_access_declarations() -> Result<()> {
    struct Frame<'w> {
        step: Res<'w, Step>,
        stats: ResMut<'w, Stats>,
    }
    impl SystemParam for Frame<'_> {
        type Item<'w> = Frame<'w>;
        fn access(access: &mut SystemAccess) -> Result<()> {
            <(Res<'static, Step>, ResMut<'static, Stats>) as SystemParam<()>>::access(access)
        }
        fn fetch(input: SystemInput<'_, ()>) -> Result<Self::Item<'_>> {
            let (step, stats) = <(Res<'static, Step>, ResMut<'static, Stats>)>::fetch(input)?;
            Ok(Frame { step, stats })
        }
    }
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Step(2.0))?;
    ecs.add_resource(Stats::default())?;
    ecs.add_system("update", |mut frame: Frame| {
        frame.stats.frames += frame.step.0 as usize;
    });
    ecs.run()?;
    assert_eq!(ecs.resource::<Stats>()?.frames, 2);
    rejects(|_: Frame, _: Res<Stats>| -> () { panic!("custom conflict") })
}

#[test]
fn host_resources_do_not_override_ecs_resources() -> Result<()> {
    struct Simulation {
        resources: Resources,
        name: &'static str,
    }
    let mut host = Simulation {
        resources: Resources::default(),
        name: "simulation",
    };
    host.resources.add(Step(99.0))?;
    let (_runtime, mut ecs) = support::ecs::<Simulation>()?;
    ecs.add_resource(Step(2.0))?;
    ecs.add_resource(Stats::default())?;
    let entity = ecs.world_mut().spawn_empty();
    ecs.world_mut().add_component(entity, Position(0.0))?;
    ecs.world_mut().add_component(entity, Velocity(1.0))?;
    ecs.add_system("update", integrate)
        .add_system("update", |host: &mut Simulation| {
            assert_eq!(host.name, "simulation");
            assert_eq!(host.resources.get::<Step>().unwrap().0, 99.0);
        });
    ecs.run_with(&mut host)?;
    assert_eq!(ecs.world().component::<Position>(entity)?.0, 2.0);
    assert_eq!(ecs.resource::<Stats>()?.frames, 1);
    Ok(())
}
