//! Convert typed functions through `IntoSystem::into_system()`.

use std::marker::PhantomData;

use super::{EcsContext, Res, Resource, System, SystemAccess, SystemInput, SystemParam, World};
use anyhow::Result;

/// Function systems may return either unit or the framework's fallible result.
pub trait SystemOutput {
    fn into_result(self) -> Result<()>;
}

impl SystemOutput for () {
    fn into_result(self) -> Result<()> {
        Ok(())
    }
}

impl SystemOutput for Result<()> {
    fn into_result(self) -> Result<()> {
        self
    }
}

/// Convert a function or an existing custom System into a schedulable System.
pub trait IntoSystem<C, Marker> {
    type System: System<C>;

    fn into_system(self) -> Self::System;
}

#[doc(hidden)]
pub struct IsSystem;
#[doc(hidden)]
pub struct IsFunction;
#[doc(hidden)]
pub struct IsExclusive;

impl<C, S: System<C>> IntoSystem<C, IsSystem> for S {
    type System = S;

    fn into_system(self) -> Self::System {
        self
    }
}

/// A function's inferred parameter tuple and invocation adapter.
pub trait SystemParamFunction<C: 'static, Marker>: 'static {
    type Param: SystemParam<C>;

    fn call(&mut self, params: <Self::Param as SystemParam<C>>::Item<'_>) -> Result<()>;
}

pub struct FunctionSystem<F, Marker, C> {
    function: F,
    validated: bool,
    marker: PhantomData<fn() -> (Marker, C)>,
}

impl<C: 'static, F, Marker: 'static> IntoSystem<C, (IsFunction, Marker)> for F
where
    F: SystemParamFunction<C, Marker>,
{
    type System = FunctionSystem<F, Marker, C>;

    fn into_system(self) -> Self::System {
        FunctionSystem {
            function: self,
            validated: false,
            marker: PhantomData,
        }
    }
}

impl<C: 'static, F, Marker: 'static> System<C> for FunctionSystem<F, Marker, C>
where
    F: SystemParamFunction<C, Marker>,
{
    fn run(&mut self, world: &mut World, context: &mut C, access: EcsContext<'_, C>) -> Result<()> {
        if !self.validated {
            F::Param::access(&mut SystemAccess::default())?;
            self.validated = true;
        }
        let params = F::Param::fetch(SystemInput {
            world,
            context,
            access,
        })?;
        self.function.call(params)
    }

    fn name(&self) -> &str {
        std::any::type_name::<F>()
    }
}

macro_rules! impl_functions {
    ($($p:ident),*) => {
        impl<Host: 'static, Func, Out, $($p: SystemParam<Host> + 'static),*> SystemParamFunction<Host, fn($($p),*) -> Out> for Func
        where
            Func: FnMut($($p),*) -> Out + 'static,
            for<'w> Func: FnMut($($p::Item<'w>),*) -> Out,
            Out: SystemOutput,
        {
            type Param = ($($p,)*);

            #[allow(non_snake_case)]
            fn call(&mut self, params: <Self::Param as SystemParam<Host>>::Item<'_>) -> Result<()> {
                fn call_inner<Out, $($p,)*>(mut f: impl FnMut($($p),*) -> Out, $($p: $p,)*) -> Out {
                    f($($p),*)
                }
                let ($($p,)*) = params;
                call_inner(self, $($p,)*).into_result()
            }
        }
    };
}

impl_functions!();
impl_functions!(A);
impl_functions!(A, B);
impl_functions!(A, B, C);
impl_functions!(A, B, C, D);
impl_functions!(A, B, C, D, E);
impl_functions!(A, B, C, D, E, F);
impl_functions!(A, B, C, D, E, F, G);
impl_functions!(A, B, C, D, E, F, G, H);

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
    type System = ExclusiveSystem<F, (ExclusiveWorldResource, T, Out), Host>;

    fn into_system(self) -> Self::System {
        ExclusiveSystem {
            function: self,
            marker: PhantomData,
        }
    }
}

impl<Host: 'static, T: Resource, F, Out> System<Host>
    for ExclusiveSystem<F, (ExclusiveWorldResource, T, Out), Host>
where
    F: FnMut(&mut World, Res<'_, T>) -> Out + 'static,
    Out: SystemOutput + 'static,
{
    fn run(&mut self, world: &mut World, _: &mut Host, access: EcsContext<'_, Host>) -> Result<()> {
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
            type System = ExclusiveSystem<F, ($kind, fn($($ty),*) -> Out), Host>;

            fn into_system(self) -> Self::System {
                ExclusiveSystem { function: self, marker: PhantomData }
            }
        }

        impl<Host: 'static, F, Out> System<Host> for ExclusiveSystem<F, ($kind, fn($($ty),*) -> Out), Host>
        where
            F: FnMut($($ty),*) -> Out + 'static,
            Out: SystemOutput + 'static,
        {
            fn run(&mut self, $world: &mut World, $context: &mut Host, _access: EcsContext<'_, Host>) -> Result<()> {
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
