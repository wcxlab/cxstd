mod support;

use cxstd::ecs::{Plugin, PluginContext, ResMut, Resource, System, SystemContext};
use std::{cell::Cell, rc::Rc};

#[derive(Default)]
struct Trace(Vec<usize>);
impl Resource for Trace {}

#[derive(Default)]
struct StatefulSystem {
    runs: usize,
}

impl System for StatefulSystem {
    type Deps = ResMut<'static, Trace>;

    fn run(&mut self, _ctx: SystemContext, deps: ResMut<Trace>) -> anyhow::Result<()> {
        let mut trace = deps;
        self.runs += 1;
        trace.0.push(self.runs);
        Ok(())
    }
}

struct NamedSystem;
impl System for NamedSystem {
    type Deps = ();

    fn name(&self) -> &str {
        "named-system"
    }

    fn run(&mut self, _ctx: SystemContext, _deps: ()) -> anyhow::Result<()> {
        anyhow::bail!("expected failure")
    }
}

struct SetupPlugin;
impl Plugin for SetupPlugin {
    type Deps = ();

    fn load(&mut self, ctx: PluginContext, _deps: ()) -> anyhow::Result<()> {
        ctx.add_resource(Trace::default());
        ctx.add_system("update", StatefulSystem::default());
        Ok(())
    }
}

struct NamedPlugin;
impl Plugin for NamedPlugin {
    type Deps = ();

    fn name(&self) -> &str {
        "named-plugin"
    }

    fn load(&mut self, _ctx: PluginContext, _deps: ()) -> anyhow::Result<()> {
        Ok(())
    }
}

#[test]
fn names_default_to_the_type_and_allow_overrides() {
    assert_eq!(
        System::name(&StatefulSystem::default()),
        std::any::type_name::<StatefulSystem>()
    );
    assert_eq!(
        Plugin::name(&SetupPlugin),
        std::any::type_name::<SetupPlugin>()
    );
    assert_eq!(System::name(&NamedSystem), "named-system");
    assert_eq!(Plugin::name(&NamedPlugin), "named-plugin");
}

#[test]
fn plugin_system_instances_retain_state_across_stage_runs() -> anyhow::Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let plugin = ecs.add_plugin(SetupPlugin)?;
    assert_eq!(
        plugin.read(&ecs)?.name(),
        std::any::type_name::<SetupPlugin>()
    );
    ecs.run_stage("update")?;
    ecs.run_stage("update")?;
    assert_eq!(ecs.resource::<Trace>()?.0, [1, 2]);
    Ok(())
}

#[test]
fn errors_identify_the_system_and_stop_later_instances() -> anyhow::Result<()> {
    struct Later(Rc<Cell<bool>>);
    impl System for Later {
        type Deps = ();
        fn run(&mut self, _ctx: SystemContext, _deps: ()) -> anyhow::Result<()> {
            self.0.set(true);
            Ok(())
        }
    }
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    let later = Rc::new(Cell::new(false));
    ecs.add_system("update", NamedSystem);
    ecs.add_system("update", Later(later.clone()));
    let error = format!("{:#}", ecs.run_stage("update").unwrap_err());
    assert!(error.contains("named-system"));
    assert!(error.contains("expected failure"));
    assert!(!later.get());
    Ok(())
}
