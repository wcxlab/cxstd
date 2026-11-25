//! Typed component selection through `Query<(&mut Position, &Velocity)>`.

use std::marker::PhantomData;

use anyhow::ensure;

use super::{
    Component, ComponentMut, ComponentRef, Dependences, EntityId, SystemAccess, SystemInput, World,
};

/// The component access and fetched item for one entity. Tuples combine fields
/// with AND semantics; EntityId may be included without selecting a component.
pub trait QueryData {
    type Item<'w>;

    fn access(access: &mut SystemAccess) -> anyhow::Result<()>;
    fn matches(world: &World, entity: EntityId) -> bool;
    fn fetch(world: &World, entity: EntityId) -> anyhow::Result<Self::Item<'_>>;
}

/// Query data that can be fetched repeatedly through shared Query access.
pub trait ReadOnlyQueryData: QueryData {}

/// A system's component selection. Mutable iteration borrows this Query
/// exclusively; guards keep each component borrowed until the item is dropped.
pub struct Query<'w, D: QueryData> {
    world: &'w World,
    marker: PhantomData<fn() -> D>,
}

impl<D: QueryData> Query<'_, D> {
    pub fn iter_mut(&mut self) -> impl Iterator<Item = D::Item<'_>> {
        let world = self.world;
        world.entities().filter_map(move |entity| {
            D::matches(world, entity).then(|| {
                D::fetch(world, entity)
                    .expect("validated query has no conflicting component borrow")
            })
        })
    }

    pub fn get_mut(&mut self, entity: EntityId) -> anyhow::Result<D::Item<'_>> {
        ensure!(
            self.world.contains(entity) && D::matches(self.world, entity),
            "entity does not match this query"
        );
        D::fetch(self.world, entity)
    }
}

impl<D: ReadOnlyQueryData> Query<'_, D> {
    pub fn iter(&self) -> impl Iterator<Item = D::Item<'_>> {
        let world = self.world;
        world.entities().filter_map(move |entity| {
            D::matches(world, entity).then(|| {
                D::fetch(world, entity)
                    .expect("validated query has no conflicting component borrow")
            })
        })
    }

    pub fn get(&self, entity: EntityId) -> anyhow::Result<D::Item<'_>> {
        ensure!(
            self.world.contains(entity) && D::matches(self.world, entity),
            "entity does not match this query"
        );
        D::fetch(self.world, entity)
    }
}

impl<C: 'static, D: QueryData + 'static> Dependences<C> for Query<'_, D> {
    type Item<'w> = Query<'w, D>;

    fn access(access: &mut SystemAccess) -> anyhow::Result<()> {
        D::access(access)
    }

    fn fetch(input: SystemInput<'_, C>) -> anyhow::Result<Self::Item<'_>> {
        Ok(Query {
            world: input.world,
            marker: PhantomData,
        })
    }
}

impl<C: Component> QueryData for &C {
    type Item<'w> = ComponentRef<'w, C>;

    fn access(access: &mut SystemAccess) -> anyhow::Result<()> {
        access.component::<C>(false)
    }
    fn matches(world: &World, entity: EntityId) -> bool {
        world.has_component::<C>(entity)
    }
    fn fetch(world: &World, entity: EntityId) -> anyhow::Result<Self::Item<'_>> {
        world.component::<C>(entity)
    }
}

impl<C: Component> ReadOnlyQueryData for &C {}

impl<C: Component> QueryData for &mut C {
    type Item<'w> = ComponentMut<'w, C>;

    fn access(access: &mut SystemAccess) -> anyhow::Result<()> {
        access.component::<C>(true)
    }
    fn matches(world: &World, entity: EntityId) -> bool {
        world.has_component::<C>(entity)
    }
    fn fetch(world: &World, entity: EntityId) -> anyhow::Result<Self::Item<'_>> {
        world.borrow_component_mut::<C>(entity)
    }
}

impl QueryData for EntityId {
    type Item<'w> = EntityId;

    fn access(_: &mut SystemAccess) -> anyhow::Result<()> {
        Ok(())
    }
    fn matches(world: &World, entity: EntityId) -> bool {
        world.contains(entity)
    }
    fn fetch(_: &World, entity: EntityId) -> anyhow::Result<Self::Item<'_>> {
        Ok(entity)
    }
}

impl ReadOnlyQueryData for EntityId {}

macro_rules! impl_query_data {
    ($($p:ident),*) => {
        impl<$($p: QueryData),*> QueryData for ($($p,)*) {
            type Item<'w> = ($($p::Item<'w>,)*);

            fn access(_access: &mut SystemAccess) -> anyhow::Result<()> {
                $($p::access(_access)?;)*
                Ok(())
            }

            fn matches(_world: &World, _entity: EntityId) -> bool {
                true $(&& $p::matches(_world, _entity))*
            }

            fn fetch(_world: &World, _entity: EntityId) -> anyhow::Result<Self::Item<'_>> {
                Ok(($($p::fetch(_world, _entity)?,)*))
            }
        }

        impl<$($p: ReadOnlyQueryData),*> ReadOnlyQueryData for ($($p,)*) {}
    };
}

impl_query_data!();
impl_query_data!(A);
impl_query_data!(A, B);
impl_query_data!(A, B, C);
impl_query_data!(A, B, C, D);
impl_query_data!(A, B, C, D, E);
impl_query_data!(A, B, C, D, E, F);
impl_query_data!(A, B, C, D, E, F, G);
impl_query_data!(A, B, C, D, E, F, G, H);
