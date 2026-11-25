use anyhow::Result;
use cxstd::ecs::{Ecs, ScopedResources, System, SystemContext};
use cxstd::ecs::{Res, ResMut, Resource};

struct Label(String);
impl Resource for Label {}
struct Count(u32);
impl Resource for Count {}

struct Advance;

impl System for Advance {
    type Deps = (Res<'static, Label>, ResMut<'static, Count>);

    fn run(
        &mut self,
        ctx: SystemContext,
        (name, mut count): (Res<Label>, ResMut<Count>),
    ) -> Result<()> {
        assert_eq!(name.0, "tick");
        assert!(ctx.resource::<Count>().is_err());
        assert!(ctx.resource_mut::<Label>().is_err());
        count.0 += 1;
        anyhow::ensure!(count.0 > 1, "first tick failed");
        Ok(())
    }
}

#[tokio::test]
async fn stage_borrows_end_on_success_and_failure() -> Result<()> {
    let mut ecs = Ecs::new()?;
    ecs.add_resource(Count(99))?;
    ecs.add_stage("update");
    ecs.add_system("update", Advance)?;
    let name = Label(String::from("tick"));
    let mut count = Count(0);
    for tick in 1..=2 {
        if tick == 1 {
            let mut resources = ScopedResources::default();
            resources.add(&name);
            assert!(resources.get_mut::<Label>().is_err());
            resources.add_mut(&mut count);
            let result = ecs.with_resources(resources, |ecs| ecs.run_stage("update"));
            assert!(format!("{:#}", result.unwrap_err()).contains("first tick failed"));
        } else {
            ecs.with_resources((&name, &mut count), |ecs| ecs.run_stage("update"))?;
        }
        assert_eq!(count.0, tick);
        assert_eq!(ecs.resource::<Count>()?.0, 99);
        assert!(ecs.run_stage("update").is_err());
    }
    Ok(())
}
