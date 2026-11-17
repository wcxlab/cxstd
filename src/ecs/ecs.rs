//! ECS instances through `Ecs::new()` and `ecs.run_stage("update")`.
//! Resources, tasks, targeted events, commands, entities, and named schedules
//! belong to the instance. Application dependencies can be registered as resources.

use std::{
    any::Any,
    collections::{HashMap, hash_map::Entry},
    future::Future,
};

use anyhow::{Context as _, anyhow};

use super::registration::Registrations;
use super::{
    CommandQueue, Dependences, EcsContext, EntityId, EventDispatcher, IntoSystem, Plugin,
    PluginContext, PluginRef, Res, ResMut, Resource, Resources, Schedule, Subscription,
    SystemAccess, SystemContext, SystemInput, Task, TaskExecutor, World,
};

/// An independent ECS instance. Schedules and systems retain registration order
/// and state across runs. Schedules are identified by their string names.
pub struct Ecs<C: 'static = ()> {
    world: World,
    resources: Resources,
    tasks: TaskExecutor,
    events: EventDispatcher<EntityId, World, C>,
    commands: CommandQueue<C>,
    registrations: Registrations<C>,
    plugins: HashMap<u64, Box<dyn Any>>,
    schedules: HashMap<String, Schedule<C>>,
    schedule_order: Vec<String>,
}

impl<C: 'static> Ecs<C> {
    /// Create an instance inside the host's active Tokio runtime.
    pub fn new() -> anyhow::Result<Self> {
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
            registrations: Registrations::default(),
            plugins: HashMap::new(),
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

    pub fn add_resource<T: Resource>(&mut self, resource: T) -> anyhow::Result<Option<T>> {
        self.resources.add(resource)
    }

    pub fn resource<T: Resource>(&self) -> anyhow::Result<Res<'_, T>> {
        self.resources.get::<T>()
    }

    pub fn resource_mut<T: Resource>(&self) -> anyhow::Result<ResMut<'_, T>> {
        self.resources.get_mut::<T>()
    }

    pub fn with_resource<T: Resource, R>(
        &mut self,
        run: impl FnOnce(&mut T, &mut Self) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
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

    pub fn context(&self) -> SystemContext<'_, C> {
        SystemContext {
            world: &self.world,
            access: self.facilities(),
        }
    }

    /// Load a plugin and take ownership after its setup succeeds.
    pub fn add_plugin_with<P: Plugin<C>>(
        &mut self,
        mut plugin: P,
        host: &mut C,
    ) -> anyhow::Result<PluginRef<P>> {
        self.load_plugin_with(&mut plugin, host)?;
        let handle = PluginRef::new();
        self.plugins.insert(handle.id, Box::new(plugin));
        Ok(handle)
    }

    /// Unload and destroy the referenced plugin. Failed unloads retain the instance
    /// for a retry. Registered systems and resources remain owned by this ECS.
    pub fn remove_plugin_with<P: Plugin<C>>(
        &mut self,
        handle: PluginRef<P>,
        host: &mut C,
    ) -> anyhow::Result<()> {
        let mut plugin = self
            .plugins
            .remove(&handle.id)
            .ok_or_else(|| anyhow!("plugin is not registered in this Ecs"))?
            .downcast::<P>()
            .expect("plugin type matches its handle");
        if let Err(error) = self.unload_plugin_with(&mut *plugin, host) {
            self.plugins.insert(handle.id, plugin);
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn plugin<P: Plugin<C>>(&self, handle: PluginRef<P>) -> anyhow::Result<&P> {
        Ok(self
            .plugins
            .get(&handle.id)
            .ok_or_else(|| anyhow!("plugin is not registered in this Ecs"))?
            .downcast_ref::<P>()
            .expect("plugin type matches its handle"))
    }

    pub(super) fn plugin_mut<P: Plugin<C>>(
        &mut self,
        handle: PluginRef<P>,
    ) -> anyhow::Result<&mut P> {
        Ok(self
            .plugins
            .get_mut(&handle.id)
            .ok_or_else(|| anyhow!("plugin is not registered in this Ecs"))?
            .downcast_mut::<P>()
            .expect("plugin type matches its handle"))
    }

    /// Extract dependencies, load the plugin once, then apply its setup registrations.
    /// Failed loads discard pending registrations; direct mutations through ResMut remain.
    fn load_plugin_with<P: Plugin<C>>(
        &mut self,
        plugin: &mut P,
        host: &mut C,
    ) -> anyhow::Result<()> {
        let registrations = Registrations::default();
        {
            P::Deps::access(&mut SystemAccess::default())?;
            let mut access = self.facilities();
            access.registrations = Some(&registrations);
            let deps = P::Deps::fetch(SystemInput {
                world: &self.world,
                context: host,
                access,
            })?;
            plugin.load(
                PluginContext {
                    system: SystemContext {
                        world: &self.world,
                        access,
                    },
                    registrations: &registrations,
                },
                deps,
            )?;
        }
        registrations.apply(self)?;
        self.apply_commands(host)?;
        Ok(())
    }

    fn unload_plugin_with<P: Plugin<C>>(
        &mut self,
        plugin: &mut P,
        host: &mut C,
    ) -> anyhow::Result<()> {
        let registrations = Registrations::default();
        {
            let mut access = self.facilities();
            access.registrations = Some(&registrations);
            plugin.unload(PluginContext {
                system: SystemContext {
                    world: &self.world,
                    access,
                },
                registrations: &registrations,
            })?;
        }
        registrations.apply(self)?;
        self.apply_commands(host)?;
        Ok(())
    }
}

impl<C: 'static> Ecs<C> {
    fn facilities(&self) -> EcsContext<'_, C> {
        EcsContext {
            resources: &self.resources,
            tasks: &self.tasks,
            events: &self.events,
            commands: &self.commands,
            registrations: Some(&self.registrations),
        }
    }

    fn apply_registrations(&mut self) -> anyhow::Result<()> {
        self.registrations.clone().apply(self)
    }
}

impl<C: 'static> Ecs<C> {
    /// Background completion is applied on the owner thread at a command barrier.
    pub fn spawn<F, O>(
        &self,
        future: F,
        callback: impl FnOnce(O, &mut World, &mut C, EcsContext<'_, C>) -> anyhow::Result<()> + 'static,
    ) -> anyhow::Result<Task<O>>
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
        callback: impl FnMut(&E, &mut World, &mut C) -> anyhow::Result<()> + 'static,
    ) -> Subscription {
        self.context().subscribe(target, callback)
    }

    pub fn observe(
        &self,
        target: EntityId,
        callback: impl FnMut(&mut World, &mut C) -> anyhow::Result<()> + 'static,
    ) -> Subscription {
        self.context().observe(target, callback)
    }

    pub fn emit<E: Any>(&self, target: EntityId, event: E) {
        self.events.emit(target, event);
    }
    pub fn notify(&self, target: EntityId) {
        self.events.notify(target);
    }

    pub fn apply_commands(&mut self, host: &mut C) -> anyhow::Result<usize> {
        self.commands.apply(
            &mut self.world,
            host,
            EcsContext {
                resources: &self.resources,
                tasks: &self.tasks,
                events: &self.events,
                commands: &self.commands,
                registrations: Some(&self.registrations),
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
    pub fn flush_with(&mut self, host: &mut C) -> anyhow::Result<()> {
        self.apply_registrations()?;
        super::r#async::start_tasks(&mut self.world, &self.tasks)?;
        self.tasks.update();
        self.apply_commands(host)?;
        self.dispatch_events(host);
        self.apply_commands(host)?;
        self.apply_registrations()?;
        Ok(())
    }
}

impl<C: 'static> Ecs<C> {
    /// Register an empty schedule. Re-registration preserves its systems and order.
    pub fn add_schedule(&mut self, name: impl Into<String>) -> &mut Self {
        self.schedule_mut(name);
        self
    }

    /// Add a system to the selected stage.
    pub fn add_system<M>(
        &mut self,
        stage: impl Into<String>,
        system: impl IntoSystem<C, M>,
    ) -> &mut Self {
        self.schedule_mut(stage).add_system(system);
        self
    }

    pub fn add_apply_deferred(&mut self, name: impl Into<String>) -> &mut Self {
        self.schedule_mut(name).add_apply_deferred();
        self
    }

    /// Stop at the first failure without rolling back completed changes.
    pub fn run_with(&mut self, host: &mut C) -> anyhow::Result<()> {
        let access = EcsContext {
            resources: &self.resources,
            tasks: &self.tasks,
            events: &self.events,
            commands: &self.commands,
            registrations: Some(&self.registrations),
        };
        for name in &self.schedule_order {
            self.schedules
                .get_mut(name)
                .expect("registered schedule exists")
                .run(&mut self.world, host, access)
                .with_context(|| format!("schedule {name:?} failed"))?;
        }
        self.apply_registrations()
    }

    /// Advance facilities and execute the selected stage's schedule.
    /// Unknown stages fail before any facilities or systems are advanced.
    pub fn run_stage_with(&mut self, stage: impl AsRef<str>, host: &mut C) -> anyhow::Result<()> {
        let stage = stage.as_ref();
        if !self.schedules.contains_key(stage) {
            return Err(anyhow!("stage {stage:?} is not registered"));
        }
        self.flush_with(host)?;
        let access = EcsContext {
            resources: &self.resources,
            tasks: &self.tasks,
            events: &self.events,
            commands: &self.commands,
            registrations: Some(&self.registrations),
        };
        self.schedules
            .get_mut(stage)
            .expect("registered schedule exists")
            .run(&mut self.world, host, access)
            .with_context(|| format!("stage {stage:?} failed"))?;
        self.apply_registrations()
    }
}

impl Ecs<()> {
    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> anyhow::Result<PluginRef<P>> {
        self.add_plugin_with(plugin, &mut ())
    }

    pub fn remove_plugin<P: Plugin>(&mut self, plugin: PluginRef<P>) -> anyhow::Result<()> {
        self.remove_plugin_with(plugin, &mut ())
    }

    pub fn flush(&mut self) -> anyhow::Result<()> {
        self.flush_with(&mut ())
    }
}

impl Ecs<()> {
    /// Run every registered schedule using the instance's resources and facilities.
    pub fn run(&mut self) -> anyhow::Result<()> {
        self.run_with(&mut ())
    }

    /// Advance facilities, then run the selected stage once. Commands queued by systems
    /// require an explicit barrier or a later flush or stage run.
    pub fn run_stage(&mut self, stage: impl AsRef<str>) -> anyhow::Result<()> {
        self.run_stage_with(stage, &mut ())
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

impl<C: 'static> Ecs<C> {
    pub(crate) fn add_registered_system(
        &mut self,
        stage: String,
        system: super::system::SystemRunner<C>,
    ) {
        self.schedule_mut(stage).add_registered_system(system);
    }
}

impl<C: 'static> Drop for Ecs<C> {
    fn drop(&mut self) {
        self.tasks.close();
        self.events.retain_targets(|_| false);
        self.commands.clear();
        self.registrations.clear();
    }
}
