use cxstd::ecs::Ecs;

pub fn ecs<C: 'static>() -> anyhow::Result<(tokio::runtime::Runtime, Ecs<C>)> {
    let runtime = tokio::runtime::Runtime::new()?;
    let ecs = {
        let _entered = runtime.enter();
        Ecs::new()?
    };
    Ok((runtime, ecs))
}
