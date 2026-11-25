use std::{
    any::Any,
    cell::{Ref, RefMut},
    ops::{Deref, DerefMut},
};

/// Marks additional data that can be selected by a component query.
pub trait Component: Any {}

/// A shared borrow of one entity's component.
pub struct ComponentRef<'w, C: Component>(pub(super) Ref<'w, C>);

impl<C: Component> Deref for ComponentRef<'_, C> {
    type Target = C;

    fn deref(&self) -> &C {
        &self.0
    }
}

/// An exclusive borrow of one entity's component.
pub struct ComponentMut<'w, C: Component> {
    value: ComponentMutValue<'w, C>,
}

enum ComponentMutValue<'w, C> {
    Exclusive(&'w mut C),
    Guard(RefMut<'w, C>),
}

impl<'w, C: Component> ComponentMut<'w, C> {
    pub(super) fn exclusive(value: &'w mut C) -> Self {
        Self {
            value: ComponentMutValue::Exclusive(value),
        }
    }

    pub(super) fn borrowed(value: RefMut<'w, C>) -> Self {
        Self {
            value: ComponentMutValue::Guard(value),
        }
    }
}

impl<C: Component> Deref for ComponentMut<'_, C> {
    type Target = C;

    fn deref(&self) -> &C {
        match &self.value {
            ComponentMutValue::Exclusive(value) => value,
            ComponentMutValue::Guard(value) => value,
        }
    }
}

impl<C: Component> DerefMut for ComponentMut<'_, C> {
    fn deref_mut(&mut self) -> &mut C {
        match &mut self.value {
            ComponentMutValue::Exclusive(value) => value,
            ComponentMutValue::Guard(value) => value,
        }
    }
}
