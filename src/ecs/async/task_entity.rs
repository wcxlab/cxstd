//! Build `TaskEntity::new(future)`, optionally add components, then pass it to
//! `world.add_entity(task)`. ECS flushes start and advance foreground work.

use std::{
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
};

use super::{Task, TaskExecutor};
use crate::ecs::{Component, Entity, World};

/// An entity owning an asynchronous operation. Its output is discarded.
/// Despawning the entity cancels its task. Completed entities remain until despawned.
pub struct TaskEntity {
    entity: Entity,
}

impl TaskEntity {
    pub fn new<F, O>(future: F) -> Self
    where
        F: Future<Output = O> + 'static,
        O: 'static,
    {
        let mut entity = Entity::new();
        entity.add_component(TaskState {
            future: Some(Box::pin(async move {
                let _ = future.await;
            })),
            task: None,
        });
        Self { entity }
    }
}

impl Deref for TaskEntity {
    type Target = Entity;

    fn deref(&self) -> &Self::Target {
        &self.entity
    }
}

impl DerefMut for TaskEntity {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entity
    }
}

impl From<TaskEntity> for Entity {
    fn from(task: TaskEntity) -> Self {
        task.entity
    }
}

struct TaskState {
    future: Option<Pin<Box<dyn Future<Output = ()>>>>,
    task: Option<Task<()>>,
}
impl Component for TaskState {}

pub(crate) fn start_tasks(world: &mut World, executor: &TaskExecutor) -> anyhow::Result<()> {
    for (_, mut state) in world.query_mut::<TaskState>() {
        if let Some(future) = state.future.take() {
            state.task = Some(executor.spawn_local(future)?);
        }
    }
    Ok(())
}
