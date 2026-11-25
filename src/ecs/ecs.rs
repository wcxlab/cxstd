//! ECS instances through `Ecs::new()` and `ecs.run_stage("update")`.
//! Resources, tasks, targeted events, commands, entities, and named stages
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
    SystemAccess, SystemContext, SystemInput, TaskExecutor, TaskRef, World,
};

/// An independent ECS instance. Stages and systems retain registration order
/// and state across runs. Each named stage owns a Schedule.
pub struct Ecs<C: 'static = ()> {
    world: World,
    resources: Resources,
    tasks: TaskExecutor,
    events: EventDispatcher<EntityId, World, C>,
    commands: CommandQueue<C>,
    registrations: Registrations<C>,
    plugins: HashMap<u64, Box<dyn Any>>,
    stages: HashMap<String, Schedule<C>>,
    stage_order: Vec<String>,
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
            stages: HashMap::new(),
            stage_order: Vec::new(),
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

    pub fn resource<T: Resource + ?Sized>(&self) -> anyhow::Result<Res<'_, T>> {
        self.resources.get::<T>()
    }

    pub fn resource_mut<T: Resource + ?Sized>(&self) -> anyhow::Result<ResMut<'_, T>> {
        self.resources.get_mut::<T>()
    }

    /// Lend external resources to the stages run by this closure.
    /// Accepts `ScopedResources` or a tuple of shared and mutable resource references.
    pub fn with_resources<'a, R>(
        &mut self,
        resources: impl Into<super::ScopedResources<'a>>,
        run: impl FnOnce(&mut super::ScopedEcs<'_, C>) -> R,
    ) -> R {
        let resources = resources.into();
        run(&mut super::ScopedEcs {
            ecs: self,
            resources: &resources,
        })
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

    /// Initialize a plugin and take ownership after its setup succeeds.
    pub fn add_plugin_with<P: Plugin<C>>(
        &mut self,
        mut plugin: P,
        host: &mut C,
    ) -> anyhow::Result<PluginRef<P>> {
        self.init_plugin_with(&mut plugin, host)?;
        let handle = PluginRef::new();
        self.plugins.insert(handle.id, Box::new(plugin));
        Ok(handle)
    }

    /// Clean up and destroy the referenced plugin. Failed cleanup retains the instance
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
        if let Err(error) = self.cleanup_plugin_with(&mut *plugin, host) {
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

    /// Extract dependencies, initialize the plugin once, then apply its setup registrations.
    /// Failed initialization discards pending registrations; direct mutations through ResMut remain.
    fn init_plugin_with<P: Plugin<C>>(
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
            plugin.init(
                PluginContext {
                    system: SystemContext {
                        world: &self.world,
                        access,
                    },
                    registrations: &registrations,
                    stages: &self.stages,
                },
                deps,
            )?;
        }
        registrations.apply(self)?;
        self.apply_commands(host)?;
        Ok(())
    }

    fn cleanup_plugin_with<P: Plugin<C>>(
        &mut self,
        plugin: &mut P,
        host: &mut C,
    ) -> anyhow::Result<()> {
        let registrations = Registrations::default();
        {
            let mut access = self.facilities();
            access.registrations = Some(&registrations);
            plugin.cleanup(PluginContext {
                system: SystemContext {
                    world: &self.world,
                    access,
                },
                registrations: &registrations,
                stages: &self.stages,
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
            scoped: None,
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
    pub fn add_task<F, O>(
        &self,
        future: F,
        callback: impl FnOnce(O, &mut World, &mut C, EcsContext<'_, C>) -> anyhow::Result<()> + 'static,
    ) -> anyhow::Result<TaskRef>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        self.context().add_task(future, callback)
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
        self.apply_commands_scoped(host, None)
    }

    fn apply_commands_scoped(
        &mut self,
        host: &mut C,
        scoped: Option<&super::ScopedResources<'_>>,
    ) -> anyhow::Result<usize> {
        self.commands.apply(
            &mut self.world,
            host,
            EcsContext {
                resources: &self.resources,
                scoped,
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

    /// Advance facilities without running stages.
    pub fn flush_with(&mut self, host: &mut C) -> anyhow::Result<()> {
        self.flush_scoped(host, None)
    }

    fn flush_scoped(
        &mut self,
        host: &mut C,
        scoped: Option<&super::ScopedResources<'_>>,
    ) -> anyhow::Result<()> {
        self.apply_registrations()?;
        super::r#async::start_tasks(&mut self.world, &self.tasks)?;
        self.tasks.update();
        self.apply_commands_scoped(host, scoped)?;
        self.dispatch_events(host);
        self.apply_commands_scoped(host, scoped)?;
        self.apply_registrations()?;
        Ok(())
    }
}

impl<C: 'static> Ecs<C> {
    /// Register an empty stage. Re-registration preserves its systems and order.
    pub fn add_stage(&mut self, stage: impl Into<String>) -> &mut Self {
        if let Entry::Vacant(entry) = self.stages.entry(stage.into()) {
            self.stage_order.push(entry.key().clone());
            entry.insert(Schedule::default());
        }
        self
    }

    /// Add a system to a previously registered stage.
    pub fn add_system<M>(
        &mut self,
        stage: impl Into<String>,
        system: impl IntoSystem<C, M>,
    ) -> anyhow::Result<()> {
        self.stage_mut(&stage.into())?.add_system(system);
        Ok(())
    }

    pub fn add_apply_deferred(&mut self, stage: impl Into<String>) -> anyhow::Result<()> {
        self.stage_mut(&stage.into())?.add_apply_deferred();
        Ok(())
    }

    /// Stop at the first failure without rolling back completed changes.
    pub fn run_with(&mut self, host: &mut C) -> anyhow::Result<()> {
        let access = EcsContext {
            resources: &self.resources,
            scoped: None,
            tasks: &self.tasks,
            events: &self.events,
            commands: &self.commands,
            registrations: Some(&self.registrations),
        };
        for stage in &self.stage_order {
            self.stages
                .get_mut(stage)
                .expect("registered stage exists")
                .run(&mut self.world, host, access)
                .with_context(|| format!("stage {stage:?} failed"))?;
        }
        self.apply_registrations()
    }

    /// Advance facilities and execute the selected stage's schedule.
    /// Unknown stages fail before any facilities or systems are advanced.
    pub fn run_stage_with(&mut self, stage: impl AsRef<str>, host: &mut C) -> anyhow::Result<()> {
        self.run_stage_scoped(stage.as_ref(), host, None)
    }

    pub(super) fn run_stage_scoped(
        &mut self,
        stage: &str,
        host: &mut C,
        scoped: Option<&super::ScopedResources<'_>>,
    ) -> anyhow::Result<()> {
        if !self.stages.contains_key(stage) {
            return Err(anyhow!("stage {stage:?} is not registered"));
        }
        self.flush_scoped(host, scoped)?;
        let access = EcsContext {
            resources: &self.resources,
            scoped,
            tasks: &self.tasks,
            events: &self.events,
            commands: &self.commands,
            registrations: Some(&self.registrations),
        };
        self.stages
            .get_mut(stage)
            .expect("registered stage exists")
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
    /// Run every registered stage using the instance's resources and facilities.
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
    fn stage_mut(&mut self, stage: &str) -> anyhow::Result<&mut Schedule<C>> {
        self.stages
            .get_mut(stage)
            .ok_or_else(|| anyhow!("stage {stage:?} is not registered"))
    }
}

impl<C: 'static> Ecs<C> {
    pub(crate) fn add_registered_system(
        &mut self,
        stage: String,
        system: super::system::SystemRunner<C>,
    ) -> anyhow::Result<()> {
        self.stage_mut(&stage)?.add_registered_system(system);
        Ok(())
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
