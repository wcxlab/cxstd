use std::{collections::HashMap, hash::Hash};

/// A registration identifier scoped to the registry that created it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ObserverId(usize);

struct Registration<K, H> {
    key: K,
    handler: H,
}

/// Routes caller-defined keys to handlers without executing or scheduling them.
///
/// `H` can be a callback or an external system identifier. Thread safety follows
/// `K` and `H`; the registry itself uses no reference counting or synchronization.
/// Registrations are explicitly removed by ID. IDs must not cross registries.
pub struct ObserverRegistry<K, H> {
    routes: HashMap<K, Vec<ObserverId>>,
    registrations: HashMap<ObserverId, Registration<K, H>>,
    next_id: usize,
}

impl<K: Clone + Eq + Hash, H> ObserverRegistry<K, H> {
    /// Registers a handler and returns its identifier.
    pub fn insert(&mut self, key: K, handler: H) -> ObserverId {
        let id = ObserverId(self.next_id);
        self.next_id += 1;
        self.routes.entry(key.clone()).or_default().push(id);
        self.registrations.insert(id, Registration { key, handler });
        id
    }

    /// Returns a registered handler. The runtime decides how to execute it.
    pub fn get(&self, id: ObserverId) -> Option<&H> {
        self.registrations.get(&id).map(|entry| &entry.handler)
    }

    /// Lists matching registrations in insertion order.
    ///
    /// A runtime can collect these IDs, release its registry borrow, and resolve
    /// each ID immediately before execution to honor intervening cancellations.
    pub fn matching(&self, key: &K) -> impl Iterator<Item = ObserverId> + '_ {
        self.routes.get(key).into_iter().flatten().copied()
    }

    /// Lists keys with live registrations, for host-managed lifetime cleanup.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.routes.keys()
    }

    /// Removes a registration and returns its handler to the caller.
    pub fn remove(&mut self, id: ObserverId) -> Option<H> {
        let entry = self.registrations.remove(&id)?;
        let route = self.routes.get_mut(&entry.key).unwrap();
        route.retain(|registered| *registered != id);
        if route.is_empty() {
            self.routes.remove(&entry.key);
        }
        Some(entry.handler)
    }

    /// Removes all registrations for a key, returning their handlers in insertion order.
    pub fn remove_key(&mut self, key: &K) -> Vec<H> {
        self.routes
            .remove(key)
            .unwrap_or_default()
            .into_iter()
            .map(|id| self.registrations.remove(&id).unwrap().handler)
            .collect()
    }
}

impl<K, H> Default for ObserverRegistry<K, H> {
    fn default() -> Self {
        Self {
            routes: HashMap::new(),
            registrations: HashMap::new(),
            next_id: 0,
        }
    }
}
