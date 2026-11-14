use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    hash::Hash,
    rc::Rc,
};

use crate::ecs::observer::{ObserverRegistry, Subscription};

#[derive(Clone, Eq, Hash, PartialEq)]
struct Route<K> {
    target: K,
    event_type: Option<TypeId>,
}

type Callback<A, B> = Rc<RefCell<dyn FnMut(&dyn Any, &mut A, &mut B)>>;
type Registry<K, A, B> = ObserverRegistry<Route<K>, Callback<A, B>>;

struct Pending<K> {
    route: Route<K>,
    active: Rc<Cell<bool>>,
    value: Box<dyn Any>,
}

struct State<K, A, B> {
    registry: Rc<RefCell<Registry<K, A, B>>>,
    pending: RefCell<VecDeque<Pending<K>>>,
    targets: RefCell<HashMap<K, Rc<Cell<bool>>>>,
    dispatching: Cell<bool>,
}

/// Owner-thread targeted events and notifications with host-provided execution data.
///
/// Clones share subscriptions and a FIFO queue. `dispatch()` supplies one context;
/// `dispatch_with()` supplies two independently borrowed values, such as World
/// and Context. Neither value is owned by the dispatcher. Newly queued events
/// wait for the next dispatch; nested dispatch calls do no work.
pub struct EventDispatcher<K, A = (), B = ()> {
    state: Rc<State<K, A, B>>,
}

impl<K: Clone + Eq + Hash + 'static, A: 'static> EventDispatcher<K, A> {
    pub fn subscribe<E: Any>(
        &self,
        target: K,
        mut callback: impl FnMut(&E, &mut A) + 'static,
    ) -> Subscription {
        self.subscribe_with(target, move |event, context, _| callback(event, context))
    }

    pub fn observe(&self, target: K, mut callback: impl FnMut(&mut A) + 'static) -> Subscription {
        self.observe_with(target, move |context, _| callback(context))
    }

    pub fn dispatch(&self, context: &mut A) -> usize {
        self.dispatch_with(context, &mut ())
    }

    pub fn dispatch_if(&self, context: &mut A, mut is_live: impl FnMut(&K, &A) -> bool) -> usize {
        self.dispatch_if_with(context, &mut (), |key, context, _| is_live(key, context))
    }
}

impl<K: Clone + Eq + Hash + 'static, A: 'static, B: 'static> EventDispatcher<K, A, B> {
    pub fn subscribe_with<E: Any>(
        &self,
        target: K,
        mut callback: impl FnMut(&E, &mut A, &mut B) + 'static,
    ) -> Subscription {
        self.register(
            Route {
                target,
                event_type: Some(TypeId::of::<E>()),
            },
            Rc::new(RefCell::new(
                move |event: &dyn Any, first: &mut A, second: &mut B| {
                    callback(
                        event.downcast_ref::<E>().expect("event type matches route"),
                        first,
                        second,
                    );
                },
            )),
        )
    }

    pub fn observe_with(
        &self,
        target: K,
        mut callback: impl FnMut(&mut A, &mut B) + 'static,
    ) -> Subscription {
        self.register(
            Route {
                target,
                event_type: None,
            },
            Rc::new(RefCell::new(
                move |_: &dyn Any, first: &mut A, second: &mut B| {
                    callback(first, second);
                },
            )),
        )
    }

    pub fn emit<E: Any>(&self, target: K, event: E) {
        self.enqueue(target, Some(TypeId::of::<E>()), Box::new(event));
    }

    pub fn notify(&self, target: K) {
        self.enqueue(target, None, Box::new(()));
    }

    pub fn pending_count(&self) -> usize {
        self.state.pending.borrow().len()
    }

    /// Deliver one batch, returning events/notifications processed, not callback count.
    pub fn dispatch_with(&self, first: &mut A, second: &mut B) -> usize {
        self.dispatch_if_with(first, second, |_, _, _| true)
    }

    /// Check liveness before each event and callback. The host can destroy targets
    /// during delivery without later callbacks accessing their removed state.
    pub fn dispatch_if_with(
        &self,
        first: &mut A,
        second: &mut B,
        mut is_live: impl FnMut(&K, &A, &B) -> bool,
    ) -> usize {
        if self.state.dispatching.replace(true) {
            return 0;
        }
        let _dispatch = DispatchScope(&self.state.dispatching);
        let batch = std::mem::take(&mut *self.state.pending.borrow_mut());
        let mut delivered = 0;
        for event in batch {
            if !event.active.get() || !is_live(&event.route.target, first, second) {
                continue;
            }
            let ids = self
                .state
                .registry
                .borrow()
                .matching(&event.route)
                .collect::<Vec<_>>();
            for id in ids {
                if !event.active.get() || !is_live(&event.route.target, first, second) {
                    break;
                }
                let callback = self.state.registry.borrow().get(id).cloned();
                if let Some(callback) = callback {
                    callback.borrow_mut()(&*event.value, first, second);
                }
            }
            delivered += 1;
        }
        delivered
    }

    /// Remove subscriptions and invalidate queued deliveries, including the active
    /// batch. Reusing the key starts a new target lifetime.
    pub fn remove_key(&self, target: &K) {
        if let Some(active) = self.state.targets.borrow_mut().remove(target) {
            active.set(false);
        }
        let removed_callbacks = {
            let mut registry = self.state.registry.borrow_mut();
            let routes = registry
                .keys()
                .filter(|route| &route.target == target)
                .cloned()
                .collect::<Vec<_>>();
            routes
                .into_iter()
                .flat_map(|route| registry.remove_key(&route))
                .collect::<Vec<_>>()
        };
        drop(removed_callbacks);
        let removed_events = {
            let mut pending = self.state.pending.borrow_mut();
            let (retained, removed): (VecDeque<_>, VecDeque<_>) = std::mem::take(&mut *pending)
                .into_iter()
                .partition(|event| &event.route.target != target);
            *pending = retained;
            removed
        };
        // Captures and payload destructors may enqueue new work.
        drop(removed_events);
    }

    /// Release registrations and queued payloads for targets rejected by the host.
    pub fn retain_targets(&self, mut keep: impl FnMut(&K) -> bool) {
        let targets = self
            .state
            .targets
            .borrow()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for target in targets {
            if !keep(&target) {
                self.remove_key(&target);
            }
        }
    }
}

impl<K: Clone + Eq + Hash + 'static, A: 'static, B: 'static> EventDispatcher<K, A, B> {
    fn target_token(&self, target: K) -> Rc<Cell<bool>> {
        self.state
            .targets
            .borrow_mut()
            .entry(target)
            .or_insert_with(|| Rc::new(Cell::new(true)))
            .clone()
    }

    fn enqueue(&self, target: K, event_type: Option<TypeId>, value: Box<dyn Any>) {
        let active = self.target_token(target.clone());
        self.state.pending.borrow_mut().push_back(Pending {
            route: Route { target, event_type },
            active,
            value,
        });
    }

    fn register(&self, route: Route<K>, callback: Callback<A, B>) -> Subscription {
        self.target_token(route.target.clone());
        let id = self.state.registry.borrow_mut().insert(route, callback);
        let registry = Rc::downgrade(&self.state.registry);
        Subscription::new(move || {
            if let Some(registry) = registry.upgrade() {
                let removed = registry.borrow_mut().remove(id);
                drop(removed);
            }
        })
    }
}

impl<K, A, B> Default for EventDispatcher<K, A, B> {
    fn default() -> Self {
        Self {
            state: Rc::new(State {
                registry: Rc::new(RefCell::new(ObserverRegistry::default())),
                pending: RefCell::new(VecDeque::new()),
                targets: RefCell::new(HashMap::new()),
                dispatching: Cell::new(false),
            }),
        }
    }
}

impl<K, A, B> Clone for EventDispatcher<K, A, B> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
        }
    }
}

struct DispatchScope<'a>(&'a Cell<bool>);
impl Drop for DispatchScope<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
