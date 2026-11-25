use cxstd::ecs::Ecs;
use cxstd::ecs::{Dependences, System, SystemContext, SystemOutput};
use std::marker::PhantomData;

pub fn ecs<C: 'static>() -> anyhow::Result<(tokio::runtime::Runtime, Ecs<C>)> {
    let runtime = tokio::runtime::Runtime::new()?;
    let ecs = {
        let _entered = runtime.enter();
        Ecs::new()?
    };
    Ok((runtime, ecs))
}

/// Test fixture for scenarios whose behavior is supplied by a local closure.
struct TestSystem<C, F, M> {
    run: F,
    marker: PhantomData<fn() -> (C, M)>,
}

#[allow(dead_code)]
pub fn system<C: 'static, F: TestFunction<C, M>, M: 'static>(run: F) -> impl System<C> {
    TestSystem {
        run,
        marker: PhantomData,
    }
}

impl<C: 'static, F: TestFunction<C, M>, M: 'static> System<C> for TestSystem<C, F, M> {
    type Deps = F::Deps;

    fn run<'w>(
        &mut self,
        ctx: SystemContext<'w, C>,
        deps: <Self::Deps as Dependences<C>>::Item<'w>,
    ) -> anyhow::Result<()> {
        self.run.call(ctx, deps)
    }
}

pub trait TestFunction<C: 'static, M>: 'static {
    type Deps: Dependences<C>;
    fn call<'w>(
        &mut self,
        ctx: SystemContext<'w, C>,
        deps: <Self::Deps as Dependences<C>>::Item<'w>,
    ) -> anyhow::Result<()>;
}

impl<C: 'static, D: Dependences<C> + 'static, F, O: SystemOutput>
    TestFunction<C, fn(SystemContext<'static, C>, D) -> O> for F
where
    F: FnMut(SystemContext<'_, C>, D) -> O + 'static,
    for<'w> F: FnMut(SystemContext<'w, C>, D::Item<'w>) -> O,
{
    type Deps = D;

    fn call<'w>(&mut self, ctx: SystemContext<'w, C>, deps: D::Item<'w>) -> anyhow::Result<()> {
        fn invoke<'w, C, D, O>(
            mut run: impl FnMut(SystemContext<'w, C>, D) -> O,
            ctx: SystemContext<'w, C>,
            deps: D,
        ) -> O {
            run(ctx, deps)
        }
        invoke(self, ctx, deps).into_result()
    }
}
