use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use cxstd::{
    Result,
    ecs::{Component, Entity, World},
};

struct Position(f32);
impl Component for Position {}
struct Velocity(f32);
impl Component for Velocity {}

#[test]
fn ordinary_state_can_create_children_and_survives_scoped_errors_and_unwinding() -> Result<()> {
    struct Panel {
        count: u32,
    }
    let mut world = World::default();
    let panel = world.spawn(Panel { count: 0 });
    let child = panel.update(&mut world, |state, world| {
        state.count += 1;
        assert!(panel.read(world).is_err());
        Ok(world.spawn(String::from("child")))
    })?;
    assert_eq!(&**child.read(&world)?, "child");
    assert_eq!(panel.read(&world)?.count, 1);
    assert!(
        panel
            .update(&mut world, |state, _| -> Result<()> {
                state.count += 1;
                anyhow::bail!("expected failure")
            })
            .is_err()
    );
    assert_eq!(panel.read(&world)?.count, 2);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = panel.update(&mut world, |state, _| -> Result<()> {
                state.count += 1;
                panic!("expected unwind");
            });
        }))
        .is_err()
    );
    assert_eq!(panel.read(&world)?.count, 3);
    assert!(world.typed::<String>(panel.entity()).is_err());
    assert!(panel.read(&World::default()).is_err());
    Ok(())
}

#[test]
fn despawn_invalidates_all_handles_and_releases_state() -> Result<()> {
    struct State(Rc<()>);
    let mut world = World::default();
    let lifetime = Rc::new(());
    let state = world.spawn(State(lifetime.clone()));
    let copied = state;
    assert_eq!(Rc::strong_count(&lifetime), 2);
    state.update(&mut world, |state_value, world| {
        assert_eq!(Rc::strong_count(&state_value.0), 2);
        assert!(world.despawn(state.entity()));
        Ok(())
    })?;
    assert_eq!(Rc::strong_count(&lifetime), 1);
    assert!(copied.read(&world).is_err());
    assert!(!world.despawn(state.entity()));
    assert_ne!(state.entity(), world.spawn_empty());
    Ok(())
}

#[test]
fn queries_filter_components_and_reject_duplicate_mutable_access() -> Result<()> {
    let mut world = World::default();
    let mut moving = Entity::new();
    assert!(moving.add_component(Position(99.0)).is_none());
    assert_eq!(moving.add_component(Position(0.0)).unwrap().0, 99.0);
    moving.add_component(Velocity(2.0));
    assert_eq!(world.entities().len(), 0);
    assert_eq!(world.query::<Position>().count(), 0);
    let moving = world.add_entity(moving);
    let stationary = world.spawn_empty();
    let unrelated = world.spawn_empty();
    world.add_component(stationary, Position(5.0))?;
    world.add_component(unrelated, Velocity(9.0))?;
    assert!(world.query2_mut::<Position, Position>().is_err());
    for _ in 0..2 {
        for (_, mut position, velocity) in world.query2_mut::<Position, Velocity>()? {
            position.0 += velocity.0 * 0.5;
        }
    }
    assert_eq!(world.component::<Position>(moving)?.0, 2.0);
    assert_eq!(world.component::<Position>(stationary)?.0, 5.0);
    assert_eq!(
        world
            .query::<Position>()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        [moving, stationary]
    );
    assert_eq!(world.add_component(moving, Position(7.0))?.unwrap().0, 2.0);
    assert_eq!(world.remove::<Position>(moving)?.unwrap().0, 7.0);
    assert!(world.component::<Position>(moving).is_err());
    assert_eq!(world.query::<Position>().count(), 1);
    Ok(())
}
