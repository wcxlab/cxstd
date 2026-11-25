//! Call `ecs.add_plugin(View2dPlugin::default())` to register
//! page entities and tree and paint systems.
//! The host supplies a Core Status snapshot before running the update schedule.
use cxstd::ecs::{
    Component, Entity, Plugin, PluginContext, Query, Res, ResMut, System, SystemContext,
};

use super::render::PaintList;
use crate::app_core::Status;

#[derive(Default)]
pub(super) struct View2dPlugin;

impl Plugin for View2dPlugin {
    type Deps = ();

    fn init(&mut self, ctx: PluginContext, _deps: ()) -> anyhow::Result<()> {
        let mut view_entity1 = Entity::new();
        view_entity1.add_component(ElementTree::new("page 1"));
        ctx.add_entity(view_entity1);

        let mut view_entity2 = Entity::new();
        view_entity2.add_component(ElementTree::new("page 2"));
        ctx.add_entity(view_entity2);

        ctx.add_system("update", BuildView2dSystem)?;
        ctx.add_system("render", PaintView2dSystem)?;
        Ok(())
    }
}

pub(super) struct BuildView2dSystem;

impl System for BuildView2dSystem {
    type Deps = (
        Query<'static, &'static mut ElementTree>,
        Res<'static, Status>,
    );

    fn run(
        &mut self,
        _ctx: SystemContext,
        deps: (Query<&mut ElementTree>, Res<Status>),
    ) -> anyhow::Result<()> {
        let (mut trees, status) = deps;
        let counter = status.counter();
        for mut tree in trees.iter_mut() {
            tree.builds += 1;
            tree.root = Some(Element {
                text: tree.name.clone(),
                children: vec![build_label(tree.builds), build_status(&status, counter)],
            });
            println!("build_view2d_system: {}, build {}", tree.name, tree.builds);
        }
        Ok(())
    }
}

pub(super) struct PaintView2dSystem;

impl System for PaintView2dSystem {
    type Deps = (
        Query<'static, &'static ElementTree>,
        ResMut<'static, PaintList>,
    );

    fn run(
        &mut self,
        _ctx: SystemContext,
        deps: (Query<&ElementTree>, ResMut<PaintList>),
    ) -> anyhow::Result<()> {
        let (trees, mut list) = deps;
        for tree in trees.iter() {
            list.0.clear();
            if let Some(root) = &tree.root {
                root.paint(&mut list.0, 0);
            }
        }
        Ok(())
    }
}

struct Element {
    text: String,
    children: Vec<Element>,
}

impl Element {
    fn paint(&self, list: &mut Vec<String>, depth: usize) {
        list.push(format!("{}{}", "  ".repeat(depth), self.text));
        for child in &self.children {
            child.paint(list, depth + 1);
        }
    }
}

pub(super) struct ElementTree {
    name: String,
    root: Option<Element>,
    builds: usize,
}
impl Component for ElementTree {}

impl ElementTree {
    pub(super) fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            root: None,
            builds: 0,
        }
    }
}

fn build_label(build: usize) -> Element {
    Element {
        text: format!("label {build}"),
        children: Vec::new(),
    }
}

fn build_status(status: &Status, counter: u64) -> Element {
    Element {
        text: format!(
            "status: current_time={:?}, update_duration={:?}, counter={counter}",
            status.current_time, status.update_duration,
        ),
        children: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_core::AppCore;
    use crate::app_view::AppView;

    #[tokio::test]
    async fn render_reads_the_latest_tree_without_accumulating_paint_commands() -> anyhow::Result<()>
    {
        let mut core = AppCore::new()?;
        let mut view = AppView::new()?;
        let pages: Vec<_> = view
            .ecs
            .world()
            .query::<ElementTree>()
            .map(|(entity, tree)| {
                assert!(tree.root.is_none());
                entity
            })
            .collect();
        assert_eq!(pages.len(), 2);

        for build in 1..=2 {
            core.update()?;
            let status = core.status()?;
            view.ecs.add_resource(status.clone())?;
            let status_command = format!(
                "  status: current_time={:?}, update_duration={:?}, counter={}",
                status.current_time,
                status.update_duration,
                status.counter(),
            );
            let previous = view.ecs.resource::<PaintList>()?.0.clone();
            view.ecs.run_stage("update")?;
            for &viewentity in &pages {
                assert_eq!(
                    view.ecs
                        .world()
                        .component::<ElementTree>(viewentity)?
                        .builds,
                    build
                );
            }
            assert_eq!(view.ecs.world().query::<ElementTree>().count(), 2);
            assert_eq!(view.ecs.resource::<PaintList>()?.0, previous);

            view.ecs.run_stage("render")?;
            assert_eq!(
                view.ecs.resource::<PaintList>()?.0,
                vec![
                    "page 1".to_string(),
                    format!("  label {build}"),
                    status_command.clone(),
                    "page 2".to_string(),
                    format!("  label {build}"),
                    status_command,
                ],
            );
        }
        Ok(())
    }
}
