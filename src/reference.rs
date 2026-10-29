//! Borrowed access through `Ref::from(&value)` and `RefMut::from(&mut value)`.
//! Both types also accept `RefCell` borrow guards and retain them until dropped.

use std::{
    cell,
    ops::{Deref, DerefMut},
};

/// Shared access to a value for the duration of a borrow.
pub struct Ref<'a, T: ?Sized> {
    inner: RefInner<'a, T>,
}

enum RefInner<'a, T: ?Sized> {
    Direct(&'a T),
    Cell(cell::Ref<'a, T>),
}

/// Exclusive access to a value for the duration of a borrow.
pub struct RefMut<'a, T: ?Sized> {
    inner: RefMutInner<'a, T>,
}

enum RefMutInner<'a, T: ?Sized> {
    Direct(&'a mut T),
    Cell(cell::RefMut<'a, T>),
}

impl<'a, T: ?Sized> From<&'a T> for Ref<'a, T> {
    fn from(value: &'a T) -> Self {
        Self {
            inner: RefInner::Direct(value),
        }
    }
}

impl<'a, T: ?Sized> From<cell::Ref<'a, T>> for Ref<'a, T> {
    fn from(value: cell::Ref<'a, T>) -> Self {
        Self {
            inner: RefInner::Cell(value),
        }
    }
}

impl<T: ?Sized> Deref for Ref<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match &self.inner {
            RefInner::Direct(value) => value,
            RefInner::Cell(value) => value,
        }
    }
}

impl<'a, T: ?Sized> From<&'a mut T> for RefMut<'a, T> {
    fn from(value: &'a mut T) -> Self {
        Self {
            inner: RefMutInner::Direct(value),
        }
    }
}

impl<'a, T: ?Sized> From<cell::RefMut<'a, T>> for RefMut<'a, T> {
    fn from(value: cell::RefMut<'a, T>) -> Self {
        Self {
            inner: RefMutInner::Cell(value),
        }
    }
}

impl<T: ?Sized> Deref for RefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match &self.inner {
            RefMutInner::Direct(value) => value,
            RefMutInner::Cell(value) => value,
        }
    }
}

impl<T: ?Sized> DerefMut for RefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        match &mut self.inner {
            RefMutInner::Direct(value) => value,
            RefMutInner::Cell(value) => value,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::{Ref, RefMut};

    #[test]
    fn cell_guards_retain_their_borrows_until_dropped() {
        let value = RefCell::new(1);
        let read = Ref::from(value.borrow());
        assert_eq!(*read, 1);
        assert!(value.try_borrow_mut().is_err());
        drop(read);

        let mut write = RefMut::from(value.borrow_mut());
        *write = 2;
        assert!(value.try_borrow().is_err());
        assert!(value.try_borrow_mut().is_err());
        drop(write);
        assert_eq!(*value.borrow(), 2);
    }

    #[test]
    fn cell_guard_releases_its_borrow_during_unwinding() {
        let value = RefCell::new(1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut write = RefMut::from(value.borrow_mut());
            *write = 2;
            panic!("test unwind");
        }));
        assert!(result.is_err());
        assert_eq!(*value.borrow(), 2);
    }
}
