use std::{
    any::{Any, TypeId, type_name},
    cell::{Ref, RefCell, RefMut},
    collections::HashMap,
    marker::PhantomData,
};

use anyhow::{Context as _, anyhow};

use super::{Ecs, Res, ResMut, Resource};

/// Host borrows available to stages run through `Ecs::with_resources`.
/// Systems inject a shared `Res<T>` or exclusive `ResMut<T>`, just like stored resources.
/// The concrete type or trait object T must be static; its borrow lasts only for the scope.
///
/// ```compile_fail
/// use cxstd::ecs::ScopedResources;
/// struct Label(String);
/// impl cxstd::ecs::Resource for Label {}
/// let mut resources = ScopedResources::default();
/// {
///     let temporary = Label(String::from("this update"));
///     resources.add(&temporary);
/// }
/// let _ = resources.get::<Label>();
/// ```
#[derive(Default)]
pub struct ScopedResources<'a> {
    slots: HashMap<TypeId, RefCell<Box<dyn Any>>>,
    lifetime: PhantomData<&'a mut ()>,
}

impl<'a> ScopedResources<'a> {
    pub fn add<T: Resource + ?Sized>(&mut self, value: &'a T) {
        self.slots
            .insert(TypeId::of::<T>(), RefCell::new(Box::new(value as *const T)));
    }

    pub fn add_mut<T: Resource + ?Sized>(&mut self, value: &'a mut T) {
        self.slots
            .insert(TypeId::of::<T>(), RefCell::new(Box::new(value as *mut T)));
    }

    pub(super) fn contains<T: Resource + ?Sized>(&self) -> bool {
        self.slots.contains_key(&TypeId::of::<T>())
    }

    pub fn get<T: Resource + ?Sized>(&self) -> anyhow::Result<Res<'_, T>> {
        let slot = self.slot::<T>()?.try_borrow()?;
        Ok(Res {
            inner: Ref::map(slot, |entry| {
                let ptr = entry
                    .downcast_ref::<*const T>()
                    .copied()
                    .unwrap_or_else(|| *entry.downcast_ref::<*mut T>().unwrap() as *const T);
                // SAFETY: insertion borrows T for 'a. The returned guard borrows the
                // table and holds the slot's shared borrow for its entire lifetime.
                unsafe { &*ptr }
            }),
        })
    }

    pub fn get_mut<T: Resource + ?Sized>(&self) -> anyhow::Result<ResMut<'_, T>> {
        let slot = self.slot::<T>()?.try_borrow_mut()?;
        Ok(ResMut {
            inner: RefMut::filter_map(slot, |entry| {
                let ptr = *entry.downcast_ref::<*mut T>()?;
                // SAFETY: only add_mut stores this pointer type, retaining the
                // exclusive host borrow. The guard prevents any aliasing slot access.
                Some(unsafe { &mut *ptr })
            })
            .map_err(|_| anyhow!("scoped resource {} is read-only", type_name::<T>()))?,
        })
    }

    fn slot<T: Resource + ?Sized>(&self) -> anyhow::Result<&RefCell<Box<dyn Any>>> {
        self.slots
            .get(&TypeId::of::<T>())
            .with_context(|| format!("scoped resource {} is absent", type_name::<T>()))
    }
}

trait ScopedResourceRef<'a> {
    fn add_to(self, resources: &mut ScopedResources<'a>);
}

impl<'a, T: Resource + ?Sized> ScopedResourceRef<'a> for &'a T {
    fn add_to(self, resources: &mut ScopedResources<'a>) {
        resources.add(self);
    }
}

impl<'a, T: Resource + ?Sized> ScopedResourceRef<'a> for &'a mut T {
    fn add_to(self, resources: &mut ScopedResources<'a>) {
        resources.add_mut(self);
    }
}

macro_rules! impl_resource_tuple {
    ($($ty:ident: $index:tt),+) => {
        impl<'a, $($ty: ScopedResourceRef<'a>),+> From<($($ty,)+)> for ScopedResources<'a> {
            fn from(references: ($($ty,)+)) -> Self {
                let mut resources = Self::default();
                $(references.$index.add_to(&mut resources);)+
                resources
            }
        }
    };
}

impl_resource_tuple!(A: 0);
impl_resource_tuple!(A: 0, B: 1);
impl_resource_tuple!(A: 0, B: 1, C: 2);
impl_resource_tuple!(A: 0, B: 1, C: 2, D: 3);
impl_resource_tuple!(A: 0, B: 1, C: 2, D: 3, E: 4);
impl_resource_tuple!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);
impl_resource_tuple!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6);
impl_resource_tuple!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7);

/// Stage runner borrowing both the ECS and the caller's temporary resources.
pub struct ScopedEcs<'a, C: 'static> {
    pub(super) ecs: &'a mut Ecs<C>,
    pub(super) resources: &'a ScopedResources<'a>,
}

impl<C: 'static> ScopedEcs<'_, C> {
    pub fn run_stage_with(&mut self, stage: impl AsRef<str>, host: &mut C) -> anyhow::Result<()> {
        self.ecs
            .run_stage_scoped(stage.as_ref(), host, Some(self.resources))
    }
}

impl ScopedEcs<'_, ()> {
    pub fn run_stage(&mut self, stage: impl AsRef<str>) -> anyhow::Result<()> {
        self.run_stage_with(stage, &mut ())
    }
}
