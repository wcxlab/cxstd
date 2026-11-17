//! Stateful systems declare dependencies through System::Deps.
//! Only struct systems and exclusive World/host functions can be registered.
//!
//! Ordinary functions cannot be registered:
//! ```compile_fail
//! use cxstd::ecs::{Ecs, SystemContext};
//! fn update(_: SystemContext, _: ()) {}
//! fn register(ecs: &mut Ecs) { ecs.add_system("update", update); }
//! ```
//! Ordinary closures cannot be registered:
//! ```compile_fail
//! use cxstd::ecs::{Ecs, SystemContext};
//! fn register(ecs: &mut Ecs) {
//!     ecs.add_system("update", |_: SystemContext, _: ()| {});
//! }
//! ```

use std::marker::PhantomData;

use super::{
    Dependences, EcsContext, Res, Resource, SystemAccess, SystemContext, SystemInput, World,
};

/// A persistent system whose dependencies are extracted before each run.
pub trait System<C: 'static = ()>: 'static {
    type Deps: Dependences<C>;

    /// Defaults to the Rust type name, including its module path.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    fn run<'w>(
        &mut self,
        ctx: SystemContext<'w, C>,
        deps: <Self::Deps as Dependences<C>>::Item<'w>,
    ) -> anyhow::Result<()>;
}

pub(crate) trait RunnableSystem<C> {
    fn name(&self) -> &str;
    fn run(
        &mut self,
        world: &mut World,
        host: &mut C,
        access: EcsContext<'_, C>,
    ) -> anyhow::Result<()>;
}

/// Type-erased execution adapter retained by a schedule.
#[doc(hidden)]
pub struct SystemRunner<C> {
    inner: Box<dyn RunnableSystem<C>>,
}

impl<C: 'static> SystemRunner<C> {
    fn new(system: impl RunnableSystem<C> + 'static) -> Self {
        Self {
            inner: Box::new(system),
        }
    }

    pub(crate) fn name(&self) -> &str {
        self.inner.name()
    }

    pub(crate) fn run(
        &mut self,
        world: &mut World,
        host: &mut C,
        access: EcsContext<'_, C>,
    ) -> anyhow::Result<()> {
        self.inner.run(world, host, access)
    }
}

/// Convert a struct system or an exclusive World/host function for execution.
pub trait IntoSystem<C, Marker> {
    fn into_system(self) -> SystemRunner<C>;
}

#[doc(hidden)]
pub struct IsSystem;
#[doc(hidden)]
pub struct IsRunner;
#[doc(hidden)]
pub struct IsExclusive;

impl<C: 'static, S: System<C>> IntoSystem<C, IsSystem> for S {
    fn into_system(self) -> SystemRunner<C> {
        SystemRunner::new(DeclaredSystem {
            system: self,
            validated: false,
        })
    }
}

impl<C> IntoSystem<C, IsRunner> for SystemRunner<C> {
    fn into_system(self) -> SystemRunner<C> {
        self
    }
}

struct DeclaredSystem<S> {
    system: S,
    validated: bool,
}

impl<C: 'static, S: System<C>> RunnableSystem<C> for DeclaredSystem<S> {
    fn name(&self) -> &str {
        self.system.name()
    }

    fn run(
        &mut self,
        world: &mut World,
        host: &mut C,
        access: EcsContext<'_, C>,
    ) -> anyhow::Result<()> {
        if !self.validated {
            S::Deps::access(&mut SystemAccess::default())?;
            self.validated = true;
        }
        let deps = S::Deps::fetch(SystemInput {
            world,
            context: host,
            access,
        })?;
        self.system.run(SystemContext { world, access }, deps)
    }
}

/// Exclusive functions may return unit or a fallible result.
pub trait SystemOutput {
    fn into_result(self) -> anyhow::Result<()>;
}

impl SystemOutput for () {
    fn into_result(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl SystemOutput for anyhow::Result<()> {
    fn into_result(self) -> anyhow::Result<()> {
        self
    }
}

pub struct ExclusiveSystem<F, Marker, C> {
    function: F,
    marker: PhantomData<fn() -> (Marker, C)>,
}

#[doc(hidden)]
pub struct ExclusiveWorldResource;

impl<Host: 'static, T: Resource, F, Out>
    IntoSystem<Host, (IsExclusive, ExclusiveWorldResource, T, Out)> for F
where
    F: FnMut(&mut World, Res<'_, T>) -> Out + 'static,
    Out: SystemOutput + 'static,
{
    fn into_system(self) -> SystemRunner<Host> {
        SystemRunner::new(
            ExclusiveSystem::<_, (ExclusiveWorldResource, T, Out), Host> {
                function: self,
                marker: PhantomData,
            },
        )
    }
}

impl<Host: 'static, T: Resource, F, Out> RunnableSystem<Host>
    for ExclusiveSystem<F, (ExclusiveWorldResource, T, Out), Host>
where
    F: FnMut(&mut World, Res<'_, T>) -> Out + 'static,
    Out: SystemOutput + 'static,
{
    fn run(
        &mut self,
        world: &mut World,
        _: &mut Host,
        access: EcsContext<'_, Host>,
    ) -> anyhow::Result<()> {
        let resource = access.resource::<T>()?;
        (self.function)(world, resource).into_result()
    }

    fn name(&self) -> &str {
        std::any::type_name::<F>()
    }
}

macro_rules! impl_exclusive {
    ($kind:ident, ($($ty:ty),*), |$world:ident, $context:ident| $($arg:ident),*) => {
        impl<Host: 'static, F, Out> IntoSystem<Host, (IsExclusive, $kind, fn($($ty),*) -> Out)> for F
        where
            F: FnMut($($ty),*) -> Out + 'static,
            Out: SystemOutput + 'static,
        {
            fn into_system(self) -> SystemRunner<Host> {
                SystemRunner::new(ExclusiveSystem::<_, ($kind, fn($($ty),*) -> Out), Host> { function: self, marker: PhantomData })
            }
        }

        impl<Host: 'static, F, Out> RunnableSystem<Host> for ExclusiveSystem<F, ($kind, fn($($ty),*) -> Out), Host>
        where
            F: FnMut($($ty),*) -> Out + 'static,
            Out: SystemOutput + 'static,
        {
            fn run(&mut self, $world: &mut World, $context: &mut Host, _access: EcsContext<'_, Host>) -> anyhow::Result<()> {
                (self.function)($($arg),*).into_result()
            }

            fn name(&self) -> &str { std::any::type_name::<F>() }
        }
    };
}

#[doc(hidden)]
pub struct ExclusiveBoth;
#[doc(hidden)]
pub struct ExclusiveContextWorld;
#[doc(hidden)]
pub struct ExclusiveWorld;
#[doc(hidden)]
pub struct ExclusiveContext;

impl_exclusive!(
    ExclusiveBoth,
    (&mut World, &mut Host),
    |world, context| world,
    context
);
impl_exclusive!(
    ExclusiveContextWorld,
    (&mut Host, &mut World),
    |world, context| context,
    world
);
impl_exclusive!(ExclusiveWorld, (&mut World), |world, _context| world);
impl_exclusive!(ExclusiveContext, (&mut Host), |_world, context| context);
