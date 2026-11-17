mod support;
use cxstd::{
    Resource,
    ecs::{
        CommandQueue, Component, Entity, EntityId, EventDispatcher, Query, Res, ResMut,
        Subscription, World,
    },
};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

struct Position(u32);
impl Component for Position {}

#[derive(Default)]
struct Ids(Vec<EntityId>);
impl Resource for Ids {}

#[test]
fn host_resource_handles_share_injection_and_preserve_independent_borrows() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let resources = ecs.resources().clone();
    resources.add(Ids::default())?;
    let read = resources.get::<Ids>()?;
    struct Count(u32);
    impl Resource for Count {}
    resources.add(Count(1))?;
    assert!(resources.add(Ids::default()).is_err());
    assert!(read.0.is_empty());
    drop(read);
    ecs.add_system(
        "update",
        support::system(|_: cxstd::ecs::SystemContext<'_, _>, deps: ResMut<Count>| {
            let mut count = deps;
            count.0 += 1
        }),
    );
    ecs.run_stage("update")?;
    assert_eq!(resources.get::<Count>()?.0, 2);
    let mut scoped = resources.scope::<Count>()?;
    assert!(ecs.resource::<Count>().is_err());
    scoped.0 = 3;
    drop(scoped);
    assert_eq!(ecs.resource::<Count>()?.0, 3);
    Ok(())
}

#[test]
fn entity_commands_reserve_ids_and_apply_in_order_at_barriers() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Ids::default())?;
    let mut original = Entity::new();
    original.add_component(Position(5));
    let original = ecs.world_mut().add_entity(original);
    ecs.add_system(
        "build",
        support::system(
            move |_: cxstd::ecs::SystemContext<'_, _>,
                  deps: (CommandQueue, ResMut<Ids>, Query<&Position>)| {
                let (ctx, mut ids, positions) = deps;
                assert_eq!(positions.iter().count(), 1);
                let parent = ctx.add_state(String::from("persistent state"));
                let child = ctx.add_entity(Entity::new());
                ctx.add_component(parent.entity(), Position(3));
                ctx.add_component(child, Position(4));
                ctx.remove::<Position>(original);
                ids.0.extend([parent.entity(), child]);
                ctx.queue(move |world, _| {
                    assert_eq!(parent.read(world)?.as_str(), "persistent state");
                    Ok(())
                });
            },
        ),
    );
    ecs.add_system(
        "build",
        support::system(
            move |_: cxstd::ecs::SystemContext<'_, _>, deps: (&World, Res<Ids>)| {
                let (world, ids) = deps;
                assert!(!world.contains(ids.0[0]));
                assert!(!world.contains(ids.0[1]));
                assert_eq!(world.component::<Position>(original).unwrap().0, 5);
            },
        ),
    );
    ecs.add_apply_deferred("build");
    ecs.add_system(
        "after",
        support::system(
            |_: cxstd::ecs::SystemContext<'_, _>,
             deps: (CommandQueue, Res<Ids>, Query<&Position>)| {
                let (ctx, ids, positions) = deps;
                assert_eq!(positions.iter().map(|position| position.0).sum::<u32>(), 7);
                ctx.despawn(ids.0[1]);
            },
        ),
    );
    ecs.run()?;
    let ids = ecs.resource::<Ids>()?.0.clone();
    assert!(ecs.world().contains(ids[0]));
    assert!(ecs.world().contains(ids[1]));
    assert!(ecs.world().component::<Position>(original).is_err());
    ecs.apply_commands(&mut ())?;
    assert!(ecs.world().contains(ids[0]));
    assert!(!ecs.world().contains(ids[1]));
    Ok(())
}

struct Target(EntityId);
impl Resource for Target {}
#[derive(Default)]
struct Subscriptions(Vec<Subscription>);
impl Resource for Subscriptions {}

#[test]
fn context_delivers_tasks_typed_events_and_change_notifications() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let target = ecs.world_mut().add_entity(Entity::with_state(0_u32));
    let target = ecs.world_mut().typed::<u32>(target)?;
    ecs.add_resource(Target(target.entity()))?;
    ecs.add_resource(Subscriptions::default())?;
    let changes = Rc::new(Cell::new(0));
    let observed = changes.clone();
    ecs.add_system(
        "update",
        support::system(
            move |_: cxstd::ecs::SystemContext<'_, _>,
                  deps: (
                &cxstd::ecs::TaskExecutor,
                &EventDispatcher<EntityId, World>,
                Res<Target>,
                ResMut<Subscriptions>,
            )|
                  -> anyhow::Result<()> {
                let (tasks, events, target_id, mut keep) = deps;
                let notify = events.clone();
                keep.0.push(
                    events.subscribe_with(target_id.0, move |value: &u32, world, _| {
                        *world.state_mut(target).unwrap() = *value;
                        notify.notify(target.entity());
                    }),
                );
                let observed = observed.clone();
                keep.0
                    .push(events.observe_with(target_id.0, move |world, _| {
                        assert_eq!(*target.read(world).unwrap(), 9);
                        observed.set(observed.get() + 1);
                    }));
                let events = events.clone();
                tasks
                    .spawn_local(async move {
                        events.emit(target.entity(), 9_u32);
                    })?
                    .detach();
                Ok(())
            },
        ),
    );
    ecs.run()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while *target.read(ecs.world())? != 9 {
        anyhow::ensure!(Instant::now() < deadline, "completion timed out");
        ecs.flush()?;
        std::thread::yield_now();
    }
    assert_eq!(changes.get(), 0);
    ecs.flush()?;
    assert_eq!(changes.get(), 1);
    Ok(())
}

#[test]
fn direct_context_resource_access_respects_existing_mutable_guards() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Ids::default())?;
    let resources = ecs.resources().clone();
    ecs.add_system(
        "update",
        support::system(
            move |_: cxstd::ecs::SystemContext<'_, _>, deps: ResMut<Ids>| {
                let _ = deps;
                assert!(resources.get::<Ids>().is_err());
                assert!(resources.get_mut::<Ids>().is_err());
            },
        ),
    );
    ecs.run()?;
    assert!(ecs.resource_mut::<Ids>().is_ok());
    Ok(())
}
