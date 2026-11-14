//! ECS facilities aggregated for internal execution and host adapters.
//! Entity changes are deferred until a command barrier; event delivery and task
//! progression are advanced by the owning Ecs. Context borrows these facilities.

use std::{any::Any, future::Future};

use super::{
    CommandQueue, Component, EntityId, EntityRef, EventDispatcher, Res, ResMut, Resource,
    Resources, Subscription, Task, TaskExecutor, World,
};
use anyhow::Result;

/// Borrowed facilities for ECS execution and host adapters. Systems inject
/// individual capabilities instead of this aggregate. Direct resource lookups
/// here use runtime borrow checks.
pub struct EcsContext<'w, C = ()> {
    pub(crate) resources: &'w Resources,
    pub(crate) tasks: &'w TaskExecutor,
    pub(crate) events: &'w EventDispatcher<EntityId, World, C>,
    pub(crate) commands: &'w CommandQueue<C>,
}

impl<C> Copy for EcsContext<'_, C> {}
impl<C> Clone for EcsContext<'_, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'w, C: 'static> EcsContext<'w, C> {
    /// Borrow the same facilities through host-owned shared handles.
    pub fn new(
        resources: &'w Resources,
        tasks: &'w TaskExecutor,
        events: &'w EventDispatcher<EntityId, World, C>,
        commands: &'w CommandQueue<C>,
    ) -> Self {
        Self {
            resources,
            tasks,
            events,
            commands,
        }
    }

    pub fn resources(&self) -> &'w Resources {
        self.resources
    }
    pub fn resource<T: Resource>(&self) -> Result<Res<'w, T>> {
        self.resources.get::<T>()
    }
    pub fn resource_mut<T: Resource>(&self) -> Result<ResMut<'w, T>> {
        self.resources.get_mut::<T>()
    }
    pub fn task_executor(&self) -> &'w TaskExecutor {
        self.tasks
    }
    pub fn event_dispatcher(&self) -> &'w EventDispatcher<EntityId, World, C> {
        self.events
    }
    pub fn command_queue(&self) -> CommandQueue<C> {
        self.commands.clone()
    }

    /// Queue a structural change without holding a World borrow.
    pub fn queue(&self, run: impl FnOnce(&mut World) -> Result<()> + 'static) {
        self.commands.queue(move |world, _| run(world));
    }

    pub fn queue_with(
        &self,
        run: impl FnOnce(&mut World, &mut C, EcsContext<'_, C>) -> Result<()> + 'static,
    ) {
        self.commands.queue_with(run);
    }

    /// Reserve an identity now. The entity becomes accessible when commands apply.
    pub fn spawn_empty(&self) -> EntityId {
        let entity = EntityId::allocate();
        self.queue(move |world| {
            world.materialize(entity, None);
            Ok(())
        });
        entity
    }

    /// Retain an ordinary state struct at the next command barrier.
    pub fn spawn<T: 'static>(&self, state: T) -> EntityRef<T> {
        let entity = EntityId::allocate();
        self.queue(move |world| {
            world.materialize(entity, Some(Box::new(state)));
            Ok(())
        });
        EntityRef::new(entity)
    }

    pub fn add_component<T: Component>(&self, entity: EntityId, component: T) {
        self.queue(move |world| {
            world.entity_mut(entity)?.add_component(component);
            Ok(())
        });
    }

    pub fn remove<T: Component>(&self, entity: EntityId) {
        self.queue(move |world| {
            world.remove::<T>(entity)?;
            Ok(())
        });
    }

    pub fn despawn(&self, entity: EntityId) {
        self.queue(move |world| {
            world.despawn(entity);
            Ok(())
        });
    }

    pub fn emit<E: Any>(&self, target: EntityId, event: E) {
        self.events.emit(target, event);
    }
    pub fn notify(&self, target: EntityId) {
        self.events.notify(target);
    }

    /// Callback errors are returned at the next command barrier.
    pub fn subscribe<E: Any>(
        &self,
        target: EntityId,
        mut callback: impl FnMut(&E, &mut World, &mut C) -> Result<()> + 'static,
    ) -> Subscription {
        let commands = self.commands.clone();
        self.events
            .subscribe_with(target, move |event, world, host| {
                if let Err(error) = callback(event, world, host) {
                    commands.queue(move |_, _| Err(error));
                }
            })
    }

    pub fn observe(
        &self,
        target: EntityId,
        mut callback: impl FnMut(&mut World, &mut C) -> Result<()> + 'static,
    ) -> Subscription {
        let commands = self.commands.clone();
        self.events.observe_with(target, move |world, host| {
            if let Err(error) = callback(world, host) {
                commands.queue(move |_, _| Err(error));
            }
        })
    }

    /// Queue a background completion for owner-thread application at a barrier.
    pub fn spawn_task<F, O>(
        &self,
        future: F,
        callback: impl FnOnce(O, &mut World, &mut C, EcsContext<'_, C>) -> Result<()> + 'static,
    ) -> Result<Task<O>>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let commands = self.commands.clone();
        Ok(self.tasks.spawn(future, move |output| {
            commands.queue_with(move |world, host, cx| callback(output, world, host, cx));
        })?)
    }
}
