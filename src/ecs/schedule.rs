use anyhow::Context as _;

use super::{EcsContext, IntoSystem, World};
use crate::Result;

/// A persistent unit of serial execution. Function systems inject declared parameters.
/// Systems may retain non-Send state, such as an event cursor or a UI runtime.
pub trait System<C = ()>: 'static {
    fn run(&mut self, world: &mut World, context: &mut C, access: EcsContext<'_, C>) -> Result<()>;

    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

enum Step<C> {
    System(Box<dyn System<C>>),
    ApplyDeferred,
}

/// Ordered systems and explicit command barriers for an application-owned host.
/// `Schedule<C>` retains systems across runs; `C` defaults to unit for ECS-only use.
pub struct Schedule<C = ()> {
    steps: Vec<Step<C>>,
}

impl<C> Default for Schedule<C> {
    fn default() -> Self {
        Self { steps: Vec::new() }
    }
}

impl<C: 'static> Schedule<C> {
    pub fn add_system<M>(&mut self, system: impl IntoSystem<C, M>) -> &mut Self {
        self.steps
            .push(Step::System(Box::new(system.into_system())));
        self
    }

    /// Insert a command barrier at this position. There is no implicit final flush.
    pub fn add_apply_deferred(&mut self) -> &mut Self {
        self.steps.push(Step::ApplyDeferred);
        self
    }

    /// Run in registration order, stopping at the first error without rollback.
    pub fn run(
        &mut self,
        world: &mut World,
        context: &mut C,
        access: EcsContext<'_, C>,
    ) -> Result<()> {
        for step in &mut self.steps {
            match step {
                Step::System(system) => {
                    let name = system.name().to_owned();
                    system
                        .run(world, context, access)
                        .with_context(|| format!("system {name} failed"))?;
                }
                Step::ApplyDeferred => {
                    access
                        .commands
                        .apply(world, context, access)
                        .context("command barrier failed")?;
                }
            }
        }
        Ok(())
    }
}
