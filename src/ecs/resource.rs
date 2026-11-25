//! Typed application resources through `Resources::add(value)` and `Resources::get::<T>()`.
//! Each Ecs creates a store; cloned Resources handles share it with host adapters.
//! `Res<T>` provides shared access and `ResMut<T>` provides exclusive mutable
//! access. Both retain their storage borrow for the lifetime of the guard.
//!
//! ```
//! use cxstd::ecs::{Res, ResMut, Resource, Resources};
//! struct Counter(u32);
//! impl Resource for Counter {}
//! # fn main() -> anyhow::Result<()> {
//! let mut resources = Resources::default();
//! resources.add(Counter(1))?;
//! {
//!     let counter: Res<Counter> = resources.get::<Counter>()?;
//!     assert_eq!(counter.0, 1);
//!     assert!(resources.get_mut::<Counter>().is_err());
//! }
//! {
//!     let mut counter: ResMut<Counter> = resources.get_mut::<Counter>()?;
//!     counter.0 += 1;
//!     assert!(resources.get::<Counter>().is_err());
//! }
//! {
//!     let mut counter = resources.scope::<Counter>()?;
//!     counter.0 += 1;
//!     assert!(resources.get::<Counter>().is_err());
//! }
//! assert_eq!(resources.get::<Counter>()?.0, 3);
//! # Ok(())
//! # }
//! ```

use std::{
    any::{Any, TypeId, type_name},
    cell::{Ref, RefCell, RefMut},
    ops::{Deref, DerefMut},
    rc::Rc,
};

use anyhow::{anyhow, ensure};

/// Marks a type that can be registered as a resource.
///
/// Resources may contain thread-local state; thread safety is determined by each type.
/// Storage and lookup are responsibilities of its owning Resources store.
///
/// ```
/// use cxstd::ecs::Resource;
///
/// struct Theme {
///     font_size: f32,
/// }
///
/// impl Resource for Theme {}
/// ```
pub trait Resource: 'static {}

/// Shared access to a Resource. The guard retains its storage borrow until dropped.
pub struct Res<'w, T: Resource + ?Sized> {
    pub(super) inner: Ref<'w, T>,
}

/// Exclusive access to a Resource. A system declares this parameter as `mut`
/// to update the resource through DerefMut.
pub struct ResMut<'w, T: Resource + ?Sized> {
    pub(super) inner: RefMut<'w, T>,
}

impl<T: Resource + ?Sized> Deref for Res<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: Resource + ?Sized> AsRef<T> for Res<'_, T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: Resource + ?Sized> Deref for ResMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: Resource + ?Sized> DerefMut for ResMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T: Resource + ?Sized> AsRef<T> for ResMut<'_, T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: Resource + ?Sized> AsMut<T> for ResMut<'_, T> {
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

type Slot<T> = Rc<RefCell<Option<Box<T>>>>;

/// Owner-thread resource storage, independent of ECS, tasks, and event delivery.
#[derive(Clone, Default)]
pub struct Resources {
    values: Rc<elsa::FrozenMap<TypeId, Box<dyn Any>>>,
}

impl Resources {
    /// Clones share this store. Adding another type does not invalidate active guards.
    pub fn add<T: Resource>(&self, resource: T) -> anyhow::Result<Option<T>> {
        let previous = if let Some(slot) = self.find_slot::<T>() {
            let mut slot = slot
                .try_borrow_mut()
                .map_err(|_| anyhow!("resource {} is already borrowed", type_name::<T>()))?;
            ensure!(
                slot.is_some(),
                "resource {} is currently scoped",
                type_name::<T>()
            );
            slot.replace(Box::new(resource))
        } else {
            self.values.insert(
                TypeId::of::<T>(),
                Box::new(Rc::new(RefCell::new(Some(Box::new(resource))))),
            );
            None
        };
        Ok(previous.map(|value| *value))
    }

    pub fn contains<T: Resource + ?Sized>(&self) -> bool {
        self.values.get(&TypeId::of::<T>()).is_some()
    }

    pub fn get<T: Resource + ?Sized>(&self) -> anyhow::Result<Res<'_, T>> {
        let value = self
            .slot::<T>()?
            .try_borrow()
            .map_err(|_| anyhow!("resource {} is already mutably borrowed", type_name::<T>()))?;
        Ok(Res {
            inner: std::cell::Ref::filter_map(value, Option::as_deref)
                .map_err(|_| anyhow!("resource {} is currently scoped", type_name::<T>()))?,
        })
    }

    pub fn get_mut<T: Resource + ?Sized>(&self) -> anyhow::Result<ResMut<'_, T>> {
        let value = self
            .slot::<T>()?
            .try_borrow_mut()
            .map_err(|_| anyhow!("resource {} is already borrowed", type_name::<T>()))?;
        Ok(ResMut {
            inner: std::cell::RefMut::filter_map(value, Option::as_deref_mut)
                .map_err(|_| anyhow!("resource {} is currently scoped", type_name::<T>()))?,
        })
    }

    /// Temporarily remove a resource without retaining a borrow of the storage.
    /// The guard restores it on drop, including errors and unwinding.
    pub fn scope<T: Resource>(&self) -> anyhow::Result<ResourceScope<T>> {
        let slot = self.slot::<T>()?;
        let value = slot
            .try_borrow_mut()
            .map_err(|_| anyhow!("resource {} is already borrowed", type_name::<T>()))?
            .take()
            .ok_or_else(|| anyhow!("resource {} is currently scoped", type_name::<T>()))?;
        Ok(ResourceScope {
            slot: slot.clone(),
            value: Some(value),
        })
    }
}

impl AsRef<Resources> for Resources {
    fn as_ref(&self) -> &Resources {
        self
    }
}

impl Resources {
    fn find_slot<T: Resource + ?Sized>(&self) -> Option<&Slot<T>> {
        self.values
            .get(&TypeId::of::<T>())?
            .downcast_ref::<Slot<T>>()
    }

    fn slot<T: Resource + ?Sized>(&self) -> anyhow::Result<&Slot<T>> {
        self.find_slot::<T>()
            .ok_or_else(|| anyhow!("resource {} is absent", type_name::<T>()))
    }
}

/// A scoped resource returned to its original storage slot when the guard drops.
pub struct ResourceScope<T: Resource> {
    slot: Slot<T>,
    value: Option<Box<T>>,
}

impl<T: Resource> Deref for ResourceScope<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value.as_deref().unwrap()
    }
}

impl<T: Resource> DerefMut for ResourceScope<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_deref_mut().unwrap()
    }
}

impl<T: Resource> Drop for ResourceScope<T> {
    fn drop(&mut self) {
        *self.slot.borrow_mut() = Some(self.value.take().unwrap());
    }
}
