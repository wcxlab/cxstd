use std::{
    any::{Any, TypeId, type_name},
    collections::BTreeMap,
};

use anyhow::{anyhow, bail, ensure};

use super::{Component, Entity, EntityId, EntityRef};
use crate::{Ref, RefMut};
use anyhow::Result;

type Value = Box<dyn Any>;

/// Entity state and queryable components, independent of application resources.
/// Entity iteration is deterministic and follows allocation order. World owns
/// state lifetimes; typed handles do not keep despawned state alive.
#[derive(Default)]
pub struct World {
    entities: BTreeMap<EntityId, Entity>,
}

impl World {
    /// Take ownership of an entity with its assembled state and components.
    pub fn add_entity(&mut self, entity: impl Into<Entity>) -> EntityId {
        let id = EntityId::allocate();
        self.materialize_entity(id, entity.into());
        id
    }

    /// Access an existing entity to add or replace its components.
    pub fn entity_mut(&mut self, entity: EntityId) -> Result<&mut Entity> {
        self.data_mut(entity)
    }

    pub fn contains(&self, entity: EntityId) -> bool {
        self.entities.contains_key(&entity)
    }

    pub fn entities(&self) -> impl ExactSizeIterator<Item = EntityId> + '_ {
        self.entities.keys().copied()
    }

    /// Remove this entity's state and components.
    pub fn despawn(&mut self, entity: EntityId) -> bool {
        self.entities.remove(&entity).is_some()
    }

    pub fn typed<T: 'static>(&self, entity: EntityId) -> Result<EntityRef<T>> {
        let handle = EntityRef::new(entity);
        self.state(handle)?;
        Ok(handle)
    }

    pub fn state<T: 'static>(&self, entity: EntityRef<T>) -> Result<Ref<'_, T>> {
        let value = self
            .data(entity.entity())?
            .state
            .as_ref()
            .ok_or_else(|| anyhow!("state is absent or currently scoped"))?;
        Ok(value
            .downcast_ref::<T>()
            .ok_or_else(|| anyhow!("entity state is not {}", type_name::<T>()))?
            .into())
    }

    pub fn state_mut<T: 'static>(&mut self, entity: EntityRef<T>) -> Result<RefMut<'_, T>> {
        let value = self
            .data_mut(entity.entity())?
            .state
            .as_mut()
            .ok_or_else(|| anyhow!("state is absent or currently scoped"))?;
        Ok(value
            .downcast_mut::<T>()
            .ok_or_else(|| anyhow!("entity state is not {}", type_name::<T>()))?
            .into())
    }

    /// Temporarily scope one state out of storage so the callback can access
    /// the World and create children. The guard restores state on errors or panic.
    pub fn with_state<T: 'static, R>(
        &mut self,
        entity: EntityRef<T>,
        update: impl FnOnce(&mut T, &mut World) -> Result<R>,
    ) -> Result<R> {
        self.state(entity)?;
        let value = self.data_mut(entity.entity())?.state.take();
        let mut scope = StateScope {
            world: self,
            entity: entity.entity(),
            value,
        };
        let state = scope.value.as_mut().unwrap().downcast_mut::<T>().unwrap();
        update(state, scope.world)
    }

    pub fn remove<C: Component>(&mut self, entity: EntityId) -> Result<Option<C>> {
        Ok(self
            .data_mut(entity)?
            .components
            .remove(&TypeId::of::<C>())
            .map(|value| {
                *value
                    .into_inner()
                    .downcast::<C>()
                    .expect("component type matches its key")
            }))
    }

    pub fn component<C: Component>(&self, entity: EntityId) -> Result<Ref<'_, C>> {
        let value = self
            .data(entity)?
            .components
            .get(&TypeId::of::<C>())
            .ok_or_else(|| anyhow!("entity has no component {}", type_name::<C>()))?
            .try_borrow()
            .map_err(|_| anyhow!("component {} is already mutably borrowed", type_name::<C>()))?;
        Ok(std::cell::Ref::map(value, |value| {
            value
                .downcast_ref::<C>()
                .expect("component type matches its key")
        })
        .into())
    }

    pub fn component_mut<C: Component>(&mut self, entity: EntityId) -> Result<RefMut<'_, C>> {
        Ok(self
            .data_mut(entity)?
            .components
            .get_mut(&TypeId::of::<C>())
            .and_then(|value| value.get_mut().downcast_mut::<C>())
            .ok_or_else(|| anyhow!("entity has no component {}", type_name::<C>()))?
            .into())
    }

    pub fn query<C: Component>(&self) -> impl Iterator<Item = (EntityId, Ref<'_, C>)> {
        self.entities.keys().copied().filter_map(|entity| {
            self.has_component::<C>(entity).then(|| {
                (
                    entity,
                    self.component::<C>(entity)
                        .expect("query component is readable"),
                )
            })
        })
    }

    pub fn query_mut<C: Component>(&mut self) -> impl Iterator<Item = (EntityId, RefMut<'_, C>)> {
        self.entities.iter_mut().filter_map(|(&entity, data)| {
            let value = data
                .components
                .get_mut(&TypeId::of::<C>())?
                .get_mut()
                .downcast_mut::<C>()?;
            Some((entity, value.into()))
        })
    }

    /// Select entities with two distinct component types, without unsafe aliasing.
    pub fn query2_mut<A: Component, B: Component>(
        &mut self,
    ) -> Result<impl Iterator<Item = (EntityId, RefMut<'_, A>, RefMut<'_, B>)>> {
        let a = TypeId::of::<A>();
        let b = TypeId::of::<B>();
        ensure!(
            a != b,
            "a mutable query cannot fetch the same component twice"
        );
        Ok(self.entities.iter_mut().filter_map(move |(&entity, data)| {
            let [first, second] = data.components.get_disjoint_mut([&a, &b]);
            Some((
                entity,
                first?.get_mut().downcast_mut::<A>()?.into(),
                second?.get_mut().downcast_mut::<B>()?.into(),
            ))
        }))
    }
}

impl World {
    pub(super) fn materialize_entity(&mut self, id: EntityId, entity: Entity) {
        self.entities.insert(id, entity);
    }

    /// Materialize an identity reserved by the process-wide entity allocator.
    pub(super) fn materialize(&mut self, entity: EntityId, state: Option<Value>) {
        self.entities.insert(
            entity,
            Entity {
                state,
                ..Entity::new()
            },
        );
    }

    pub(super) fn has_component<C: Component>(&self, entity: EntityId) -> bool {
        self.entities
            .get(&entity)
            .is_some_and(|data| data.components.contains_key(&TypeId::of::<C>()))
    }

    pub(super) fn borrow_component_mut<C: Component>(
        &self,
        entity: EntityId,
    ) -> Result<RefMut<'_, C>> {
        let value = self
            .data(entity)?
            .components
            .get(&TypeId::of::<C>())
            .ok_or_else(|| anyhow!("entity has no component {}", type_name::<C>()))?
            .try_borrow_mut()
            .map_err(|_| anyhow!("component {} is already borrowed", type_name::<C>()))?;
        Ok(std::cell::RefMut::map(value, |value| {
            value
                .downcast_mut::<C>()
                .expect("component type matches its key")
        })
        .into())
    }

    fn data(&self, entity: EntityId) -> Result<&Entity> {
        self.entities
            .get(&entity)
            .ok_or_else(|| anyhow!("entity {entity:?} does not exist in this World"))
    }

    fn data_mut(&mut self, entity: EntityId) -> Result<&mut Entity> {
        match self.entities.get_mut(&entity) {
            Some(data) => Ok(data),
            None => bail!("entity {entity:?} does not exist in this World"),
        }
    }
}

struct StateScope<'a> {
    world: &'a mut World,
    entity: EntityId,
    value: Option<Value>,
}

impl Drop for StateScope<'_> {
    fn drop(&mut self) {
        if let Some(data) = self.world.entities.get_mut(&self.entity) {
            data.state = self.value.take();
        }
    }
}
