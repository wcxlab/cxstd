//! ECS instances through `Ecs::new()` and `ecs.run_schedule("update")`.
//! Resources, tasks, targeted events, commands, entities, and named schedules
//! belong to the instance. Application dependencies can be registered as resources.

use std::{
    any::Any,
    collections::{HashMap, hash_map::Entry},
    future::Future,
};

use anyhow::{Context as _, anyhow};

use super::{
    CommandQueue, EcsContext, EntityId, EventDispatcher, IntoSystem, Res, ResMut, Resource,
    Resources, Schedule, Subscription, Task, TaskExecutor, World,
};
use anyhow::Result;

/// An independent ECS instance. Schedules and systems retain registration order
/// and state across runs. Schedules are identified by their string names.
pub struct Ecs<C: 'static = ()> {
    world: World,
    resources: Resources,
    tasks: TaskExecutor,
    events: EventDispatcher<EntityId, World, C>,
    commands: CommandQueue<C>,
    schedules: HashMap<String, Schedule<C>>,
    schedule_order: Vec<String>,
}

impl<C: 'static> Ecs<C> {
    /// Create an instance inside the host's active Tokio runtime.
    pub fn new() -> Result<Self> {
        Ok(Self::with_task_executor(TaskExecutor::new()?))
    }

    /// Adopt an existing owner-thread executor, sharing its task queue with host adapters.
    pub fn with_task_executor(tasks: TaskExecutor) -> Self {
        Self {
            world: World::default(),
            resources: Resources::default(),
            tasks,
            events: EventDispatcher::default(),
            commands: CommandQueue::default(),
            schedules: HashMap::new(),
            schedule_order: Vec::new(),
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
    pub fn resources(&self) -> &Resources {
        &self.resources
    }
    pub fn resources_mut(&mut self) -> &mut Resources {
        &mut self.resources
    }

    pub fn add_resource<T: Resource>(&mut self, resource: T) -> Result<Option<T>> {
        self.resources.add(resource)
    }

    pub fn resource<T: Resource>(&self) -> Result<Res<'_, T>> {
        self.resources.get::<T>()
    }

    pub fn resource_mut<T: Resource>(&self) -> Result<ResMut<'_, T>> {
        self.resources.get_mut::<T>()
    }

    pub fn with_resource<T: Resource, R>(
        &mut self,
        run: impl FnOnce(&mut T, &mut Self) -> Result<R>,
    ) -> Result<R> {
        let mut resource = self.resources.scope::<T>()?;
        run(&mut resource, self)
    }

    pub fn task_executor(&self) -> &TaskExecutor {
        &self.tasks
    }
    pub fn event_dispatcher(&self) -> &EventDispatcher<EntityId, World, C> {
        &self.events
    }
    pub fn command_queue(&self) -> CommandQueue<C> {
        self.commands.clone()
    }

    pub fn context(&self) -> EcsContext<'_, C> {
        EcsContext {
            resources: &self.resources,
            tasks: &self.tasks,
            events: &self.events,
            commands: &self.commands,
        }
    }

    /// Background completion is applied on the owner thread at a command barrier.
    pub fn spawn<F, O>(
        &self,
        future: F,
        callback: impl FnOnce(O, &mut World, &mut C, EcsContext<'_, C>) -> Result<()> + 'static,
    ) -> Result<Task<O>>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        self.context().spawn_task(future, callback)
    }

    /// Callback errors are returned by the next command barrier.
    pub fn subscribe<E: Any>(
        &self,
        target: EntityId,
        callback: impl FnMut(&E, &mut World, &mut C) -> Result<()> + 'static,
    ) -> Subscription {
        self.context().subscribe(target, callback)
    }

    pub fn observe(
        &self,
        target: EntityId,
        callback: impl FnMut(&mut World, &mut C) -> Result<()> + 'static,
    ) -> Subscription {
        self.context().observe(target, callback)
    }

    pub fn emit<E: Any>(&self, target: EntityId, event: E) {
        self.events.emit(target, event);
    }
    pub fn notify(&self, target: EntityId) {
        self.events.notify(target);
    }

    pub fn apply_commands(&mut self, host: &mut C) -> Result<usize> {
        self.commands.apply(
            &mut self.world,
            host,
            EcsContext {
                resources: &self.resources,
                tasks: &self.tasks,
                events: &self.events,
                commands: &self.commands,
            },
        )
    }

    /// Remove dead-target registrations before and after one event batch.
    pub fn dispatch_events(&mut self, host: &mut C) -> usize {
        self.events
            .retain_targets(|entity| self.world.contains(*entity));
        let delivered = self
            .events
            .dispatch_if_with(&mut self.world, host, |entity, world, _| {
                world.contains(*entity)
            });
        self.events
            .retain_targets(|entity| self.world.contains(*entity));
        delivered
    }

    /// Advance facilities without running schedules.
    pub fn flush_with(&mut self, host: &mut C) -> Result<()> {
        super::r#async::start_tasks(&mut self.world, &self.tasks)?;
        self.tasks.update();
        self.apply_commands(host)?;
        self.dispatch_events(host);
        self.apply_commands(host)?;
        Ok(())
    }
}

impl<C: 'static> Ecs<C> {
    /// Register an empty schedule. Re-registration preserves its systems and order.
    pub fn add_schedule(&mut self, name: impl Into<String>) -> &mut Self {
        self.schedule_mut(name);
        self
    }

    pub fn add_system<M>(
        &mut self,
        name: impl Into<String>,
        system: impl IntoSystem<C, M>,
    ) -> &mut Self {
        self.schedule_mut(name).add_system(system);
        self
    }

    pub fn add_apply_deferred(&mut self, name: impl Into<String>) -> &mut Self {
        self.schedule_mut(name).add_apply_deferred();
        self
    }

    /// Stop at the first failure without rolling back completed changes.
    pub fn run_with(&mut self, host: &mut C) -> Result<()> {
        let access = EcsContext {
            resources: &self.resources,
            tasks: &self.tasks,
            events: &self.events,
            commands: &self.commands,
        };
        for name in &self.schedule_order {
            self.schedules
                .get_mut(name)
                .expect("registered schedule exists")
                .run(&mut self.world, host, access)
                .with_context(|| format!("schedule {name:?} failed"))?;
        }
        Ok(())
    }

    /// Advance facilities and execute the schedule identified by its name.
    /// Unknown names fail before any facilities or systems are advanced.
    pub fn run_schedule_with(&mut self, name: impl AsRef<str>, host: &mut C) -> Result<()> {
        let name = name.as_ref();
        if !self.schedules.contains_key(name) {
            return Err(anyhow!("schedule {name:?} is not registered"));
        }
        self.flush_with(host)?;
        let access = EcsContext {
            resources: &self.resources,
            tasks: &self.tasks,
            events: &self.events,
            commands: &self.commands,
        };
        self.schedules
            .get_mut(name)
            .expect("registered schedule exists")
            .run(&mut self.world, host, access)
            .with_context(|| format!("schedule {name:?} failed"))
    }
}

impl Ecs<()> {
    pub fn flush(&mut self) -> Result<()> {
        self.flush_with(&mut ())
    }
}

impl Ecs<()> {
    /// Run every registered schedule using the instance's resources and facilities.
    pub fn run(&mut self) -> Result<()> {
        self.run_with(&mut ())
    }

    /// Advance facilities, then run the selected schedule once. Commands queued by systems
    /// require an explicit barrier or a later flush or schedule run.
    pub fn run_schedule(&mut self, name: impl AsRef<str>) -> Result<()> {
        self.run_schedule_with(name, &mut ())
    }
}

impl<C: 'static> Ecs<C> {
    fn schedule_mut(&mut self, name: impl Into<String>) -> &mut Schedule<C> {
        match self.schedules.entry(name.into()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                self.schedule_order.push(entry.key().clone());
                entry.insert(Schedule::default())
            }
        }
    }
}

impl<C: 'static> Drop for Ecs<C> {
    fn drop(&mut self) {
        self.tasks.close();
        self.events.retain_targets(|_| false);
        self.commands.clear();
    }
}
