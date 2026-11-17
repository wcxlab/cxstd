//! Assemble an entity with `Entity::new()` and `entity.add_component(component)`, then
//! transfer it into a World with `world.add_entity(entity)` to obtain its EntityId.

use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::HashMap,
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};

use super::World;
use crate::Ref;

static NEXT_ENTITY: AtomicU64 = AtomicU64::new(1);

/// A process-wide identity, never reused or confused with another World's entity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId(u64);

impl EntityId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl EntityId {
    pub(super) fn allocate() -> Self {
        Self(NEXT_ENTITY.fetch_add(1, Ordering::Relaxed))
    }
}

/// Marks additional data that can be selected by a component query.
pub trait Component: Any {}

/// Owns state and components before and after transfer into a World.
/// World allocates an EntityId when it takes ownership of the entity.
#[derive(Default)]
pub struct Entity {
    pub(super) state: Option<Box<dyn Any>>,
    pub(super) components: HashMap<TypeId, RefCell<Box<dyn Any>>>,
}

impl Entity {
    pub fn new() -> Self {
        Self::default()
    }

    /// Retain an ordinary struct as the entity's primary state.
    pub fn with_state<T: 'static>(state: T) -> Self {
        Self {
            state: Some(Box::new(state)),
            ..Self::new()
        }
    }

    /// Add a component, returning the previous value of the same type, if any.
    pub fn add_component<C: Component>(&mut self, component: C) -> Option<C> {
        self.components
            .insert(TypeId::of::<C>(), RefCell::new(Box::new(component)))
            .map(|value| {
                *value
                    .into_inner()
                    .downcast::<C>()
                    .expect("component type matches its key")
            })
    }
}

/// A typed identity for an entity's primary state; it does not own a second value.
/// Dropping a handle does not despawn the entity. World destruction or despawn
/// invalidates access through every copy of the handle.
pub struct EntityRef<T> {
    entity: EntityId,
    marker: PhantomData<fn() -> T>,
}

impl<T: 'static> EntityRef<T> {
    pub fn entity(self) -> EntityId {
        self.entity
    }

    pub fn read(self, world: &World) -> anyhow::Result<Ref<'_, T>> {
        world.state(self)
    }

    /// Borrow this state while allowing access to other entities.
    /// Accessing this same state recursively returns an error. The state is
    /// restored during unwinding, or dropped if the callback despawns its entity.
    pub fn update<R>(
        self,
        world: &mut World,
        update: impl FnOnce(&mut T, &mut World) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        world.with_state(self, update)
    }
}

impl<T> EntityRef<T> {
    pub(super) fn new(entity: EntityId) -> Self {
        Self {
            entity,
            marker: PhantomData,
        }
    }
}

impl<T> Copy for EntityRef<T> {}

impl<T> Clone for EntityRef<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> std::fmt::Debug for EntityRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("EntityRef").field(&self.entity).finish()
    }
}

impl<T> PartialEq for EntityRef<T> {
    fn eq(&self, other: &Self) -> bool {
        self.entity == other.entity
    }
}

impl<T> Eq for EntityRef<T> {}
