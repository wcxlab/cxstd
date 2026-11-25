//! Registrations through `ObserverRegistry::insert()` and local callbacks through
//! `Observers::subscribe()`, `trigger()`, `observe()`, and `notify()`.
//! The registry stores arbitrary handlers, including system identifiers, without
//! executing them. The local adapter executes callbacks with a caller-supplied context.
//! Dropping a `Subscription` cancels its callback; `Subscription::detach()` keeps it.
//! `Observers::remove_key()` removes an object's subscriptions, including detached ones.
//! Scheduling, weak object handles, object destruction, and redraws belong to the host.
//! Buffered streams with independent readers live in `crate::ecs::event`.
//! `crate::ecs::EventDispatcher` combines these callbacks with queued delivery.
//!
//! ```
//! use cxstd::ecs::Observers;
//!
//! let observers = Observers::<u64, Vec<u32>>::default();
//! let subscription = observers.subscribe(1, |value: &u32, context| context.push(*value));
//! let mut context = Vec::new();
//! observers.trigger(1, &42_u32, &mut context);
//! assert_eq!(context, [42]);
//! drop(subscription);
//! ```

mod observers;
mod registry;
mod subscription;

pub use observers::Observers;
pub use registry::{ObserverId, ObserverRegistry};
pub use subscription::Subscription;
