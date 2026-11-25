//! PluginContext and SystemContext expose separate setup and execution capabilities.
//! `EcsContext` carries borrowed facilities for internal execution and host adapters.
//! Entity changes are deferred until a command barrier; event delivery and task
//! progression are advanced by the owning Ecs. Context borrows these facilities.

use std::{
    any::Any,
    future::Future,
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use super::registration::{Registration, Registrations};
use super::{
    CommandQueue, Component, Entity, EntityId, EntityRef, EventDispatcher, IntoSystem, Res, ResMut,
    Resource, Resources, Subscription, TaskExecutor, TaskRef, World,
};

/// Borrowed facilities for ECS execution and host adapters. Systems inject
/// individual capabilities instead of this aggregate. Direct resource lookups
/// here use runtime borrow checks.
pub struct EcsContext<'w, C = ()> {
    pub(crate) resources: &'w Resources,
    pub(crate) scoped: Option<&'w super::ScopedResources<'w>>,
    pub(crate) tasks: &'w TaskExecutor,
    pub(crate) events: &'w EventDispatcher<EntityId, World, C>,
    pub(crate) commands: &'w CommandQueue<C>,
    pub(crate) registrations: Option<&'w Registrations<C>>,
}

/// First argument of System::run after self. Entity changes are queued;
/// dynamic resource and component lookups use runtime borrow checks.
///
/// Systems cannot register other systems:
/// ```compile_fail
/// use cxstd::ecs::{SystemContext, Res};
/// fn update(ctx: SystemContext, _deps: ()) {
///     ctx.add_system("update", |_: SystemContext, _deps: ()| {});
/// }
/// ```
/// The command queue remains internal:
/// ```compile_fail
/// use cxstd::ecs::{SystemContext, Entity};
/// fn update(ctx: SystemContext, _deps: ()) {
///     ctx.command_queue().add_entity(Entity::new());
/// }
/// ```
pub struct SystemContext<'w, C = ()> {
    pub(crate) world: &'w World,
    pub(crate) access: EcsContext<'w, C>,
}

impl<C> Copy for SystemContext<'_, C> {}
impl<C> Clone for SystemContext<'_, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'w, C> From<super::SystemInput<'w, C>> for SystemContext<'w, C> {
    fn from(input: super::SystemInput<'w, C>) -> Self {
        Self {
            world: input.world,
            access: input.access,
        }
    }
}

impl<'w, C: 'static> SystemContext<'w, C> {
    /// Borrow execution inputs for a domain-specific context or dependency adapter.
    pub fn input(&self, host: &'w C) -> super::SystemInput<'w, C> {
        super::SystemInput {
            world: self.world,
            context: host,
            access: self.access,
        }
    }

    pub fn world(&self) -> &'w World {
        self.world
    }

    /// Reserve an identity; transfer the entity at the next command barrier.
    pub fn add_entity(&self, entity: impl Into<Entity>) -> EntityId {
        self.access.commands.add_entity(entity)
    }

    /// Register a resource after the current schedule has released its borrows.
    pub fn add_resource<T: Resource>(&self, resource: T) -> anyhow::Result<()> {
        let registrations = self
            .access
            .registrations
            .ok_or_else(|| anyhow::anyhow!("resource registration requires an executing Ecs"))?;
        registrations.push(Registration::Resource(Box::new(move |resources| {
            resources.add(resource)?;
            Ok(())
        })));
        Ok(())
    }

    pub fn resource<T: Resource + ?Sized>(&self) -> anyhow::Result<Res<'w, T>> {
        self.access.resource()
    }
    pub fn resource_mut<T: Resource + ?Sized>(&self) -> anyhow::Result<ResMut<'w, T>> {
        self.access.resource_mut()
    }
    pub fn task_executor(&self) -> &'w TaskExecutor {
        self.access.task_executor()
    }
    pub fn event_dispatcher(&self) -> &'w EventDispatcher<EntityId, World, C> {
        self.access.event_dispatcher()
    }
    pub fn queue(&self, run: impl FnOnce(&mut World) -> anyhow::Result<()> + 'static) {
        self.access.queue(run);
    }
    pub fn queue_with(
        &self,
        run: impl FnOnce(&mut World, &mut C, EcsContext<'_, C>) -> anyhow::Result<()> + 'static,
    ) {
        self.access.queue_with(run);
    }
    pub fn add_component<T: Component>(&self, entity: EntityId, component: T) {
        self.access.add_component(entity, component);
    }
    pub fn remove<T: Component>(&self, entity: EntityId) {
        self.access.remove::<T>(entity);
    }
    pub fn despawn(&self, entity: EntityId) {
        self.access.despawn(entity);
    }
    pub fn emit<E: Any>(&self, entity: EntityId, event: E) {
        self.access.emit(entity, event);
    }
    pub fn notify(&self, entity: EntityId) {
        self.access.notify(entity);
    }
    pub fn subscribe<E: Any>(
        &self,
        entity: EntityId,
        callback: impl FnMut(&E, &mut World, &mut C) -> anyhow::Result<()> + 'static,
    ) -> Subscription {
        self.access.subscribe(entity, callback)
    }
    pub fn observe(
        &self,
        entity: EntityId,
        callback: impl FnMut(&mut World, &mut C) -> anyhow::Result<()> + 'static,
    ) -> Subscription {
        self.access.observe(entity, callback)
    }
    pub fn add_task<F, O>(
        &self,
        future: F,
        callback: impl FnOnce(O, &mut World, &mut C, EcsContext<'_, C>) -> anyhow::Result<()> + 'static,
    ) -> anyhow::Result<TaskRef>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        self.access.add_task(future, callback)
    }
}

/// First argument of plugin hooks. Setup can register resources, systems,
/// stages, and barriers, in addition to the shared execution capabilities.
pub struct PluginContext<'w, C = ()> {
    pub(crate) system: SystemContext<'w, C>,
    pub(crate) registrations: &'w Registrations<C>,
    pub(crate) stages: &'w std::collections::HashMap<String, super::Schedule<C>>,
}

impl<C> Copy for PluginContext<'_, C> {}
impl<C> Clone for PluginContext<'_, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'w, C> Deref for PluginContext<'w, C> {
    type Target = SystemContext<'w, C>;
    fn deref(&self) -> &Self::Target {
        &self.system
    }
}

impl<C: 'static> PluginContext<'_, C> {
    pub fn add_resource<T: Resource>(&self, resource: T) {
        self.registrations
            .push(Registration::Resource(Box::new(move |resources| {
                resources.add(resource)?;
                Ok(())
            })));
    }

    /// Add a system to an existing stage or one previously queued by this hook.
    pub fn add_system<M>(
        &self,
        stage: impl Into<String>,
        system: impl IntoSystem<C, M>,
    ) -> anyhow::Result<()> {
        let stage = stage.into();
        self.require_stage(&stage)?;
        self.registrations
            .push(Registration::System(stage, system.into_system()));
        Ok(())
    }

    /// Register an empty stage without requiring any systems.
    pub fn add_stage(&self, stage: impl Into<String>) {
        self.registrations.push(Registration::Stage(stage.into()));
    }

    pub fn add_apply_deferred(&self, stage: impl Into<String>) -> anyhow::Result<()> {
        let stage = stage.into();
        self.require_stage(&stage)?;
        self.registrations.push(Registration::ApplyDeferred(stage));
        Ok(())
    }

    fn require_stage(&self, stage: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.stages.contains_key(stage) || self.registrations.contains_stage(stage),
            "stage {stage:?} is not registered"
        );
        Ok(())
    }
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
            registrations: None,
            scoped: None,
        }
    }

    pub fn has_resource<T: Resource + ?Sized>(&self) -> bool {
        self.scoped.is_some_and(|scope| scope.contains::<T>()) || self.resources.contains::<T>()
    }

    /// The persistent resource store owned by this ECS.
    pub fn resources(&self) -> &'w Resources {
        self.resources
    }
    pub fn resource<T: Resource + ?Sized>(&self) -> anyhow::Result<Res<'w, T>> {
        if let Some(scoped) = self.scoped.filter(|scope| scope.contains::<T>()) {
            return scoped.get::<T>();
        }
        self.resources.get::<T>()
    }
    pub fn resource_mut<T: Resource + ?Sized>(&self) -> anyhow::Result<ResMut<'w, T>> {
        if let Some(scoped) = self.scoped.filter(|scope| scope.contains::<T>()) {
            return scoped.get_mut::<T>();
        }
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
    pub fn queue(&self, run: impl FnOnce(&mut World) -> anyhow::Result<()> + 'static) {
        self.commands.queue(move |world, _| run(world));
    }

    pub fn queue_with(
        &self,
        run: impl FnOnce(&mut World, &mut C, EcsContext<'_, C>) -> anyhow::Result<()> + 'static,
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
        mut callback: impl FnMut(&E, &mut World, &mut C) -> anyhow::Result<()> + 'static,
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
        mut callback: impl FnMut(&mut World, &mut C) -> anyhow::Result<()> + 'static,
    ) -> Subscription {
        let commands = self.commands.clone();
        self.events.observe_with(target, move |world, host| {
            if let Err(error) = callback(world, host) {
                commands.queue(move |_, _| Err(error));
            }
        })
    }

    /// Queue a background completion for owner-thread application at a barrier.
    pub fn add_task<F, O>(
        &self,
        future: F,
        callback: impl FnOnce(O, &mut World, &mut C, EcsContext<'_, C>) -> anyhow::Result<()> + 'static,
    ) -> anyhow::Result<TaskRef>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let commands = self.commands.clone();
        let canceled = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let completion = super::r#async::TaskCompletion(finished.clone());
        let canceled_in_callback = canceled.clone();
        let task = self.tasks.spawn(future, move |output| {
            commands.queue_with(move |world, host, ctx| {
                let _completion = completion;
                if canceled_in_callback.load(Ordering::Acquire) {
                    return Ok(());
                }
                callback(output, world, host, ctx)
            });
        })?;
        Ok(TaskRef::new(task, canceled, finished))
    }
}
