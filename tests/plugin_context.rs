mod support;

use cxstd::ecs::{
    Component, Entity, Plugin, PluginContext, Query, Res, ResMut, Resource, System, SystemContext,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct Settings(u32);
impl Resource for Settings {}
struct Position(u32);
impl Component for Position {}
#[derive(Default)]
struct Trace(Vec<u32>);
impl Resource for Trace {}
struct Ready;
impl Resource for Ready {}

struct Configure;

impl Plugin for Configure {
    type Deps = (Res<'static, Settings>,);

    fn load(&mut self, ctx: PluginContext, deps: (Res<Settings>,)) -> anyhow::Result<()> {
        let (settings,) = deps;
        assert_eq!(ctx.world().entities().count(), 0);
        ctx.add_resource(Settings(settings.0 + 1));
        ctx.add_resource(Trace::default());
        let mut entity = Entity::new();
        entity.add_component(Position(2));
        let entity = ctx.add_entity(entity);
        assert!(!ctx.world().contains(entity));
        ctx.add_system("update", Update);
        Ok(())
    }
}

struct Update;

impl System for Update {
    type Deps = (
        Res<'static, Settings>,
        ResMut<'static, Trace>,
        Query<'static, (cxstd::ecs::EntityId, &'static mut Position)>,
    );

    fn run(
        &mut self,
        ctx: SystemContext,
        deps: (
            Res<Settings>,
            ResMut<Trace>,
            Query<(cxstd::ecs::EntityId, &mut Position)>,
        ),
    ) -> anyhow::Result<()> {
        let (settings, mut trace, mut positions) = deps;
        assert!(ctx.resource_mut::<Trace>().is_err());
        for (entity, mut position) in positions.iter_mut() {
            assert!(ctx.world().component::<Position>(entity).is_err());
            position.0 += settings.0;
        }
        trace.0.push(settings.0);
        if trace.0.len() == 1 {
            ctx.add_resource(Ready)?;
            assert!(ctx.resource::<Ready>().is_err());
        }
        Ok(())
    }
}

#[test]
fn plugins_and_systems_share_context_without_exposing_ecs() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Settings(9))?;
    ecs.add_plugin(Configure)?;
    assert_eq!(ecs.resource::<Settings>()?.0, 10);
    assert_eq!(ecs.world().query::<Position>().next().unwrap().1.0, 2);
    ecs.run_stage("update")?;
    assert_eq!(ecs.resource::<Trace>()?.0, [10]);
    assert!(ecs.resource::<Ready>().is_ok());
    ecs.run_stage("update")?;
    assert_eq!(ecs.resource::<Trace>()?.0, [10, 10]);
    assert_eq!(ecs.world().query::<Position>().next().unwrap().1.0, 22);
    Ok(())
}

struct NeedsSettings(Rc<Cell<bool>>);
impl Plugin for NeedsSettings {
    type Deps = Res<'static, Settings>;

    fn load(&mut self, _: PluginContext, deps: Res<Settings>) -> anyhow::Result<()> {
        let _ = deps;
        self.0.set(true);
        Ok(())
    }
}

#[test]
fn missing_dependencies_fail_before_plugin_load_runs() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let called = Rc::new(Cell::new(false));
    assert!(ecs.add_plugin(NeedsSettings(called.clone())).is_err());
    assert!(!called.get());
    Ok(())
}

struct Conflicting(Rc<Cell<bool>>);
impl Plugin for Conflicting {
    type Deps = (Res<'static, Settings>, ResMut<'static, Settings>);

    fn load(
        &mut self,
        _: PluginContext,
        deps: (Res<Settings>, ResMut<Settings>),
    ) -> anyhow::Result<()> {
        let _ = deps;
        self.0.set(true);
        Ok(())
    }
}

#[test]
fn conflicting_dependencies_are_rejected_before_loading() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Settings(1))?;
    let called = Rc::new(Cell::new(false));
    let error = ecs.add_plugin(Conflicting(called.clone())).unwrap_err();
    assert!(error.to_string().contains("conflicting system access"));
    assert!(!called.get());
    Ok(())
}

struct Dropped(Rc<Cell<bool>>);
impl Resource for Dropped {}
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.set(true);
    }
}
struct Failing(Rc<Cell<bool>>);
impl Plugin for Failing {
    type Deps = ResMut<'static, Settings>;

    fn load(&mut self, ctx: PluginContext, deps: ResMut<Settings>) -> anyhow::Result<()> {
        let mut settings = deps;
        settings.0 = 7;
        ctx.add_resource(Dropped(self.0.clone()));
        ctx.add_system(
            "unused",
            support::system(|_: cxstd::ecs::SystemContext<'_, _>, _deps: ()| {}),
        );
        anyhow::bail!("expected load failure")
    }
}

#[test]
fn failed_load_discards_setup_and_releases_guards_without_rolling_back_mutations()
-> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Settings(1))?;
    let dropped = Rc::new(Cell::new(false));
    assert!(ecs.add_plugin(Failing(dropped.clone())).is_err());
    assert!(dropped.get());
    assert!(ecs.resource::<Dropped>().is_err());
    assert!(ecs.run_stage("unused").is_err());
    assert_eq!(ecs.resource_mut::<Settings>()?.0, 7);
    Ok(())
}

#[test]
fn context_registrations_remain_local_to_their_ecs() -> anyhow::Result<()> {
    let (_runtime1, mut first) = support::ecs::<()>()?;
    let (_runtime2, mut second) = support::ecs::<()>()?;
    first.context().add_resource(Settings(2))?;
    second.context().add_resource(Settings(8))?;
    first.flush()?;
    assert_eq!(first.resource::<Settings>()?.0, 2);
    assert!(second.resource::<Settings>().is_err());
    second.flush()?;
    assert_eq!(second.resource::<Settings>()?.0, 8);
    Ok(())
}

#[derive(Default)]
struct Host(u32);
struct Hosted;
impl Plugin<Host> for Hosted {
    type Deps = ();

    fn load(&mut self, ctx: PluginContext<Host>, _deps: ()) -> anyhow::Result<()> {
        ctx.queue_with(|_, host, _| {
            host.0 += 1;
            Ok(())
        });
        ctx.add_system("update", |host: &mut Host| host.0 += 1);
        Ok(())
    }

    fn unload(&mut self, ctx: PluginContext<Host>) -> anyhow::Result<()> {
        ctx.queue_with(|_, host, _| {
            host.0 += 10;
            Ok(())
        });
        Ok(())
    }
}

#[test]
fn plugin_lifecycle_supports_an_application_host() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<Host>()?;
    let mut host = Host::default();
    let plugin = ecs.add_plugin_with(Hosted, &mut host)?;
    assert_eq!(host.0, 1);
    ecs.run_stage_with("update", &mut host)?;
    assert_eq!(host.0, 2);
    ecs.remove_plugin_with(plugin, &mut host)?;
    assert_eq!(host.0, 12);
    Ok(())
}

struct Inspected;
impl Plugin for Inspected {
    type Deps = (Res<'static, Settings>, Query<'static, &'static Position>);

    fn load(
        &mut self,
        ctx: PluginContext,
        deps: (Res<Settings>, Query<&Position>),
    ) -> anyhow::Result<()> {
        let (settings, positions) = deps;
        ctx.add_resource(Trace(vec![
            positions.iter().map(|position| position.0).sum::<u32>() + settings.0,
        ]));
        Ok(())
    }
}

#[test]
fn plugin_load_can_inject_component_queries() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let mut entity = Entity::new();
    entity.add_component(Position(3));
    ecs.world_mut().add_entity(entity);
    ecs.add_resource(Settings(4))?;
    ecs.add_plugin(Inspected)?;
    assert_eq!(ecs.resource::<Trace>()?.0, [7]);
    Ok(())
}

#[derive(Default)]
struct Managed {
    value: u32,
    log: Rc<RefCell<Vec<(&'static str, u32)>>>,
    fail_unload: bool,
}

impl Plugin for Managed {
    type Deps = ();

    fn load(&mut self, _: PluginContext, _deps: ()) -> anyhow::Result<()> {
        self.log.borrow_mut().push(("load", self.value));
        Ok(())
    }

    fn unload(&mut self, _: PluginContext) -> anyhow::Result<()> {
        anyhow::ensure!(!self.fail_unload, "expected unload failure");
        self.log.borrow_mut().push(("unload", self.value));
        Ok(())
    }
}

impl Drop for Managed {
    fn drop(&mut self) {
        self.log.borrow_mut().push(("drop", self.value));
    }
}

#[test]
fn ecs_owns_plugins_and_handles_access_the_stored_instance() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let log = Rc::new(RefCell::new(Vec::new()));
    let plugin = ecs.add_plugin(Managed {
        value: 1,
        log: log.clone(),
        ..Managed::default()
    })?;
    let copy = plugin;
    assert_eq!(plugin.read(&ecs)?.value, 1);
    copy.read_mut(&mut ecs)?.value = 2;
    assert_eq!(plugin.read(&ecs)?.value, 2);
    assert_eq!(*log.borrow(), [("load", 1)]);

    ecs.remove_plugin(plugin)?;
    assert_eq!(*log.borrow(), [("load", 1), ("unload", 2), ("drop", 2)]);
    assert!(copy.read(&ecs).is_err());
    assert!(ecs.remove_plugin(copy).is_err());
    assert_eq!(log.borrow().len(), 3);
    Ok(())
}

#[test]
fn plugin_handles_distinguish_instances_and_ecs_owners() -> anyhow::Result<()> {
    let (_runtime, mut first) = support::ecs::<()>()?;
    let (_runtime2, mut second) = support::ecs::<()>()?;
    let one = first.add_plugin(Managed::default())?;
    let two = first.add_plugin(Managed {
        value: 2,
        log: Rc::default(),
        fail_unload: false,
    })?;
    let other = second.add_plugin(Managed::default())?;
    assert_ne!(one, two);
    assert!(one.read(&second).is_err());
    assert!(one.read_mut(&mut second).is_err());
    assert!(second.remove_plugin(one).is_err());
    assert!(other.read(&second).is_ok());
    first.remove_plugin(one)?;
    let replacement = first.add_plugin(Managed::default())?;
    assert_ne!(one, replacement);
    assert!(one.read(&first).is_err());
    assert_eq!(two.read(&first)?.value, 2);
    Ok(())
}

#[test]
fn failed_unload_retains_the_plugin_for_retry() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let log = Rc::new(RefCell::new(Vec::new()));
    let plugin = ecs.add_plugin(Managed {
        fail_unload: true,
        log: log.clone(),
        ..Managed::default()
    })?;
    assert!(ecs.remove_plugin(plugin).is_err());
    assert_eq!(*log.borrow(), [("load", 0)]);
    plugin.read_mut(&mut ecs)?.fail_unload = false;
    ecs.remove_plugin(plugin)?;
    assert_eq!(*log.borrow(), [("load", 0), ("unload", 0), ("drop", 0)]);
    Ok(())
}

#[test]
fn plugin_handles_do_not_keep_instances_alive_after_ecs_destruction() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let log = Rc::new(RefCell::new(Vec::new()));
    let plugin = ecs.add_plugin(Managed {
        log: log.clone(),
        ..Managed::default()
    })?;
    drop(ecs);
    assert_eq!(*log.borrow(), [("load", 0), ("drop", 0)]);
    let (_runtime2, other) = support::ecs::<()>()?;
    assert!(plugin.read(&other).is_err());
    Ok(())
}
