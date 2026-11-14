//! Typed application resources through `Resources::add(value)` and `Resources::get::<T>()`.
//! Each Ecs creates a store; cloned Resources handles share it with host adapters.
//! `Res<T>` provides shared access and `ResMut<T>` provides exclusive mutable
//! access. Both retain their storage borrow for the lifetime of the guard.
//!
//! ```
//! use anyhow::Result;
//! use cxstd::{Res, ResMut, Resource, Resources};
//! struct Counter(u32);
//! impl Resource for Counter {}
//! # fn main() -> Result<()> {
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
    cell::RefCell,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use anyhow::{anyhow, ensure};

use crate::{Ref, RefMut};
use anyhow::Result;

/// Marks a type that can be registered as a resource.
///
/// Resources may contain thread-local state; thread safety is determined by each type.
/// Storage and lookup are responsibilities of its owning Resources store.
///
/// ```
/// use cxstd::Resource;
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
    inner: Ref<'w, T>,
}

/// Exclusive access to a Resource. A system declares this parameter as `mut`
/// to update the resource through DerefMut.
pub struct ResMut<'w, T: Resource + ?Sized> {
    inner: RefMut<'w, T>,
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

type Slot = Rc<RefCell<Option<Box<dyn Any>>>>;

/// Owner-thread resource storage, independent of ECS, tasks, and event delivery.
#[derive(Clone, Default)]
pub struct Resources {
    values: Rc<elsa::FrozenMap<TypeId, Box<Slot>>>,
}

impl Resources {
    /// Clones share this store. Adding another type does not invalidate active guards.
    pub fn add<T: Resource>(&self, resource: T) -> Result<Option<T>> {
        let previous = if let Some(slot) = self.values.get(&TypeId::of::<T>()) {
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
        Ok(previous.map(|value| {
            *value
                .downcast::<T>()
                .expect("resource type matches its key")
        }))
    }

    pub fn contains<T: Resource>(&self) -> bool {
        self.values.get(&TypeId::of::<T>()).is_some()
    }

    pub fn get<T: Resource>(&self) -> Result<Res<'_, T>> {
        let value = self
            .slot::<T>()?
            .try_borrow()
            .map_err(|_| anyhow!("resource {} is already mutably borrowed", type_name::<T>()))?;
        Ok(Res {
            inner: std::cell::Ref::filter_map(value, |value| value.as_ref()?.downcast_ref::<T>())
                .map_err(|_| anyhow!("resource {} is currently scoped", type_name::<T>()))?
                .into(),
        })
    }

    pub fn get_mut<T: Resource>(&self) -> Result<ResMut<'_, T>> {
        let value = self
            .slot::<T>()?
            .try_borrow_mut()
            .map_err(|_| anyhow!("resource {} is already borrowed", type_name::<T>()))?;
        Ok(ResMut {
            inner: std::cell::RefMut::filter_map(value, |value| {
                value.as_mut()?.downcast_mut::<T>()
            })
            .map_err(|_| anyhow!("resource {} is currently scoped", type_name::<T>()))?
            .into(),
        })
    }

    /// Temporarily remove a resource without retaining a borrow of the storage.
    /// The guard restores it on drop, including errors and unwinding.
    pub fn scope<T: Resource>(&self) -> Result<ResourceScope<T>> {
        let slot = self.slot::<T>()?;
        let value = slot
            .try_borrow_mut()
            .map_err(|_| anyhow!("resource {} is already borrowed", type_name::<T>()))?
            .take()
            .ok_or_else(|| anyhow!("resource {} is currently scoped", type_name::<T>()))?;
        Ok(ResourceScope {
            slot: slot.clone(),
            value: Some(
                value
                    .downcast::<T>()
                    .expect("resource type matches its key"),
            ),
        })
    }
}

impl AsRef<Resources> for Resources {
    fn as_ref(&self) -> &Resources {
        self
    }
}

impl Resources {
    fn slot<T: Resource>(&self) -> Result<&Slot> {
        self.values
            .get(&TypeId::of::<T>())
            .ok_or_else(|| anyhow!("resource {} is absent", type_name::<T>()))
    }
}

/// A scoped resource returned to its original storage slot when the guard drops.
pub struct ResourceScope<T: Resource> {
    slot: Slot,
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
