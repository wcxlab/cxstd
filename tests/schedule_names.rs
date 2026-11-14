mod support;

use cxstd::{
    Result,
    ecs::{ResMut, Resource},
};

#[derive(Default)]
struct Trace(Vec<u8>);
impl Resource for Trace {}

#[test]
fn owned_and_borrowed_names_select_the_same_schedule_without_resetting_it() -> Result<()> {
    let (_runtime, mut ecs) = support::ecs::<()>()?;
    ecs.add_resource(Trace::default())?;
    let name = format!("view{}", 2);
    ecs.add_system(name, |mut trace: ResMut<Trace>| trace.0.push(1));
    ecs.add_system(String::from("view2"), |mut trace: ResMut<Trace>| {
        trace.0.push(2)
    });
    ecs.add_schedule("view2");
    ecs.run_schedule(String::from("view2"))?;
    ecs.run_schedule("view2")?;
    assert_eq!(ecs.resource::<Trace>()?.0, [1, 2, 1, 2]);
    assert!(ecs.run_schedule("View2").is_err());
    assert_eq!(ecs.resource::<Trace>()?.0, [1, 2, 1, 2]);
    Ok(())
}

#[test]
fn identical_schedule_names_are_local_to_each_ecs() -> Result<()> {
    let (_runtime1, mut first) = support::ecs::<()>()?;
    let (_runtime2, mut second) = support::ecs::<()>()?;
    first.add_resource(Trace::default())?;
    second.add_resource(Trace::default())?;
    first.add_system("update", |mut trace: ResMut<Trace>| trace.0.push(1));
    second.add_system("update", |mut trace: ResMut<Trace>| trace.0.push(2));
    first.run_schedule("update")?;
    assert!(second.resource::<Trace>()?.0.is_empty());
    second.run_schedule("update")?;
    assert_eq!(first.resource::<Trace>()?.0, [1]);
    assert_eq!(second.resource::<Trace>()?.0, [2]);
    Ok(())
}
