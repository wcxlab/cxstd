use std::{
    any::{Any, TypeId},
    cell::RefCell,
    hash::Hash,
    rc::Rc,
};

use super::{ObserverRegistry, Subscription};

#[derive(Clone, Eq, Hash, PartialEq)]
enum Route<K> {
    Event { key: Option<K>, event_type: TypeId },
    Change(K),
}

type Callback<C> = Rc<RefCell<dyn FnMut(&dyn Any, &mut C)>>;
type Registry<K, C> = ObserverRegistry<Route<K>, Callback<C>>;

/// A single-threaded callback adapter over `ObserverRegistry`.
///
/// Clones share registrations. Calls execute synchronously with no registry borrow
/// held while a callback runs. A host can defer calls until its own safe update point.
/// Dispatch snapshots IDs: new observers wait for the next dispatch, and cancelled
/// observers are skipped even if their IDs were already collected. A callback may
/// change registrations or trigger other callbacks, but must not invoke itself
/// recursively. Payloads and contexts need not implement `Send`, `Sync`, or `Clone`.
pub struct Observers<K, C = ()> {
    registry: Rc<RefCell<Registry<K, C>>>,
}

impl<K: Clone + Eq + Hash + 'static, C: 'static> Observers<K, C> {
    /// Subscribes to events of type `E` for one key.
    pub fn subscribe<E: Any>(
        &self,
        key: K,
        callback: impl FnMut(&E, &mut C) + 'static,
    ) -> Subscription {
        self.subscribe_event(Some(key), callback)
    }

    /// Subscribes to events of type `E` for all keys and global triggers.
    pub fn subscribe_global<E: Any>(
        &self,
        callback: impl FnMut(&E, &mut C) + 'static,
    ) -> Subscription {
        self.subscribe_event(None, callback)
    }

    /// Runs global observers first, then observers for `key`, in registration order
    /// within each group. Every callback borrows the same payload and context.
    pub fn trigger<E: Any>(&self, key: K, event: &E, context: &mut C) {
        let event_type = TypeId::of::<E>();
        self.dispatch(
            &[
                Route::Event {
                    key: None,
                    event_type,
                },
                Route::Event {
                    key: Some(key),
                    event_type,
                },
            ],
            event,
            context,
        );
    }

    /// Runs only global observers of type `E`.
    pub fn trigger_global<E: Any>(&self, event: &E, context: &mut C) {
        self.dispatch(
            &[Route::Event {
                key: None,
                event_type: TypeId::of::<E>(),
            }],
            event,
            context,
        );
    }

    /// Observes explicit state-change notifications for one key.
    ///
    /// Changes have a separate route from typed events, including events of type `()`.
    pub fn observe(&self, key: K, mut callback: impl FnMut(&mut C) + 'static) -> Subscription {
        self.register(
            Route::Change(key),
            Rc::new(RefCell::new(move |_: &dyn Any, context: &mut C| {
                callback(context);
            })),
        )
    }

    /// Runs change observers in registration order without coalescing notifications.
    pub fn notify(&self, key: K, context: &mut C) {
        self.dispatch(&[Route::Change(key)], &(), context);
    }

    /// Cancels all typed and change observers for `key`, including detached ones.
    ///
    /// Global observers remain registered. The host can call this when the
    /// corresponding object is destroyed; this adapter does not own the object.
    pub fn remove_key(&self, key: &K) {
        let removed = {
            let mut registry = self.registry.borrow_mut();
            let routes = registry
                .keys()
                .filter(|route| match route {
                    Route::Event {
                        key: Some(observed),
                        ..
                    }
                    | Route::Change(observed) => observed == key,
                    Route::Event { key: None, .. } => false,
                })
                .cloned()
                .collect::<Vec<_>>();
            routes
                .into_iter()
                .flat_map(|route| registry.remove_key(&route))
                .collect::<Vec<_>>()
        };
        // Callback captures may themselves own subscriptions to this registry.
        drop(removed);
    }
}

impl<K: Clone + Eq + Hash + 'static, C: 'static> Observers<K, C> {
    fn subscribe_event<E: Any>(
        &self,
        key: Option<K>,
        mut callback: impl FnMut(&E, &mut C) + 'static,
    ) -> Subscription {
        let route = Route::Event {
            key,
            event_type: TypeId::of::<E>(),
        };
        let callback: Callback<C> =
            Rc::new(RefCell::new(move |event: &dyn Any, context: &mut C| {
                callback(
                    event.downcast_ref::<E>().expect("event type matches route"),
                    context,
                );
            }));
        self.register(route, callback)
    }

    fn register(&self, route: Route<K>, callback: Callback<C>) -> Subscription {
        let id = self.registry.borrow_mut().insert(route, callback);
        let registry = Rc::downgrade(&self.registry);
        Subscription::new(move || {
            if let Some(registry) = registry.upgrade() {
                let removed = registry.borrow_mut().remove(id);
                drop(removed);
            }
        })
    }

    fn dispatch(&self, routes: &[Route<K>], event: &dyn Any, context: &mut C) {
        let ids = {
            let registry = self.registry.borrow();
            routes
                .iter()
                .flat_map(|route| registry.matching(route))
                .collect::<Vec<_>>()
        };
        for id in ids {
            let callback = self.registry.borrow().get(id).cloned();
            if let Some(callback) = callback {
                callback.borrow_mut()(event, context);
            }
        }
    }
}

impl<K, C> Default for Observers<K, C> {
    fn default() -> Self {
        Self {
            registry: Rc::new(RefCell::new(ObserverRegistry::default())),
        }
    }
}

impl<K, C> Clone for Observers<K, C> {
    fn clone(&self) -> Self {
        Self {
            registry: Rc::clone(&self.registry),
        }
    }
}
