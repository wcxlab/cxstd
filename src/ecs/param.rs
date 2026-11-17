//! Dependencies declare their access through `Dependences::access` and
//! acquire borrowed values through `Dependences::fetch` before the system runs.
//! Facilities come from the executing Ecs; the application host is independent.

use std::{
    any::{TypeId, type_name},
    collections::HashMap,
};

use anyhow::ensure;

use super::{
    CommandQueue, Component, EcsContext, EntityId, EventDispatcher, Res, ResMut, Resource,
    Resources, TaskExecutor, World,
};

/// Independently borrowed owners supplied when fetching function parameters.
pub struct SystemInput<'w, C> {
    pub world: &'w World,
    pub context: &'w C,
    pub access: EcsContext<'w, C>,
}

impl<C> Copy for SystemInput<'_, C> {}

impl<C> Clone for SystemInput<'_, C> {
    fn clone(&self) -> Self {
        *self
    }
}

/// A dependency type. Fetching uses safe borrowed guards; custom
/// implementations must also declare accesses so conflicts fail before execution.
pub trait Dependences<C: 'static = ()>: Sized {
    type Item<'w>;

    fn access(access: &mut SystemAccess) -> anyhow::Result<()>;
    fn fetch(input: SystemInput<'_, C>) -> anyhow::Result<Self::Item<'_>>;
}

/// Component and resource access declared by one system. Shared reads may repeat;
/// a write conflicts with every other access to the same type.
#[derive(Default)]
pub struct SystemAccess {
    components: AccessSet,
    resources: AccessSet,
}

impl SystemAccess {
    pub fn component<T: Component>(&mut self, mutable: bool) -> anyhow::Result<()> {
        self.components.add::<T>(mutable)
    }

    pub fn resource<T: Resource>(&mut self, mutable: bool) -> anyhow::Result<()> {
        self.resources.add::<T>(mutable)
    }

    pub fn read_world(&mut self) -> anyhow::Result<()> {
        self.components.read_all()
    }

    pub fn read_resources(&mut self) -> anyhow::Result<()> {
        self.resources.read_all()
    }
}

#[derive(Default)]
struct AccessSet {
    shared_all: bool,
    types: HashMap<TypeId, bool>,
}

impl AccessSet {
    fn add<T: 'static>(&mut self, mutable: bool) -> anyhow::Result<()> {
        let previous = self.types.get(&TypeId::of::<T>());
        ensure!(
            !(mutable && self.shared_all) && !previous.is_some_and(|&write| write || mutable),
            "conflicting system access to {}",
            type_name::<T>()
        );
        self.types.insert(TypeId::of::<T>(), mutable);
        Ok(())
    }

    fn read_all(&mut self) -> anyhow::Result<()> {
        ensure!(
            !self.types.values().any(|&write| write),
            "shared owner access conflicts with a mutable parameter"
        );
        self.shared_all = true;
        Ok(())
    }
}

impl<C: 'static, T: Resource> Dependences<C> for Res<'_, T> {
    type Item<'w> = Res<'w, T>;

    fn access(access: &mut SystemAccess) -> anyhow::Result<()> {
        access.resource::<T>(false)
    }

    fn fetch(input: SystemInput<'_, C>) -> anyhow::Result<Self::Item<'_>> {
        input.access.resources.get::<T>()
    }
}

impl<C: 'static, T: Resource> Dependences<C> for ResMut<'_, T> {
    type Item<'w> = ResMut<'w, T>;

    fn access(access: &mut SystemAccess) -> anyhow::Result<()> {
        access.resource::<T>(true)
    }

    fn fetch(input: SystemInput<'_, C>) -> anyhow::Result<Self::Item<'_>> {
        input.access.resources.get_mut::<T>()
    }
}

impl<C: 'static> Dependences<C> for &World {
    type Item<'w> = &'w World;

    fn access(access: &mut SystemAccess) -> anyhow::Result<()> {
        access.read_world()
    }

    fn fetch(input: SystemInput<'_, C>) -> anyhow::Result<Self::Item<'_>> {
        Ok(input.world)
    }
}

impl<C: 'static> Dependences<C> for &Resources {
    type Item<'w> = &'w Resources;

    fn access(access: &mut SystemAccess) -> anyhow::Result<()> {
        access.read_resources()
    }

    fn fetch(input: SystemInput<'_, C>) -> anyhow::Result<Self::Item<'_>> {
        Ok(input.access.resources)
    }
}

impl<C: 'static> Dependences<C> for CommandQueue<C> {
    type Item<'w> = CommandQueue<C>;

    fn access(_: &mut SystemAccess) -> anyhow::Result<()> {
        Ok(())
    }

    fn fetch(input: SystemInput<'_, C>) -> anyhow::Result<Self::Item<'_>> {
        Ok(input.access.command_queue())
    }
}

impl<C: 'static> Dependences<C> for &TaskExecutor {
    type Item<'w> = &'w TaskExecutor;

    fn access(_: &mut SystemAccess) -> anyhow::Result<()> {
        Ok(())
    }

    fn fetch(input: SystemInput<'_, C>) -> anyhow::Result<Self::Item<'_>> {
        Ok(input.access.task_executor())
    }
}

impl<C: 'static> Dependences<C> for &EventDispatcher<EntityId, World, C> {
    type Item<'w> = &'w EventDispatcher<EntityId, World, C>;

    fn access(_: &mut SystemAccess) -> anyhow::Result<()> {
        Ok(())
    }

    fn fetch(input: SystemInput<'_, C>) -> anyhow::Result<Self::Item<'_>> {
        Ok(input.access.events)
    }
}

macro_rules! impl_params {
    ($($p:ident),*) => {
        impl<Host: 'static, $($p: Dependences<Host>),*> Dependences<Host> for ($($p,)*) {
            type Item<'w> = ($($p::Item<'w>,)*);

            fn access(_access: &mut SystemAccess) -> anyhow::Result<()> {
                $($p::access(_access)?;)*
                Ok(())
            }

            fn fetch(_input: SystemInput<'_, Host>) -> anyhow::Result<Self::Item<'_>> {
                Ok(($($p::fetch(_input)?,)*))
            }
        }
    };
}

impl_params!();
impl_params!(A);
impl_params!(A, B);
impl_params!(A, B, C);
impl_params!(A, B, C, D);
impl_params!(A, B, C, D, E);
impl_params!(A, B, C, D, E, F);
impl_params!(A, B, C, D, E, F, G);
impl_params!(A, B, C, D, E, F, G, H);
