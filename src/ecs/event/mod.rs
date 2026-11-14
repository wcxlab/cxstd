//! Buffered events through `Events::write()` and `EventCursor::read()`.
//! Each cursor tracks its own progress without consuming another reader's events.
//! `Events::update()` rotates two buffers; events expire after two updates,
//! regardless of whether every reader has seen them. The caller controls updates.
//! Buffers have no callbacks, entity model, scheduler, or mandatory thread bounds.
//! A cursor must be used with the same buffer throughout its lifetime.
//!
//! Queued callbacks use `EventDispatcher::emit()` and `dispatch(&mut context)`.
//! The dispatcher owns targeted subscriptions, notifications, and a shared queue.
//! Its host controls dispatch boundaries independently of ECS schedules.
//! `dispatch_with(&mut first, &mut second)` supplies two independent execution
//! values to callbacks registered with `subscribe_with()` or `observe_with()`.
//!
//! ```
//! use cxstd::ecs::Events;
//!
//! let mut events = Events::<u32>::default();
//! let mut first = events.get_cursor();
//! let mut second = events.get_cursor();
//! events.write(42);
//! assert_eq!(first.read(&events).copied().collect::<Vec<_>>(), [42]);
//! assert_eq!(second.read(&events).copied().collect::<Vec<_>>(), [42]);
//! assert_eq!(first.read(&events).count(), 0);
//! ```
//!
//! ```
//! use cxstd::ecs::EventDispatcher;
//!
//! let events = EventDispatcher::<u64, Vec<u32>>::default();
//! let subscription = events.subscribe(1, |value: &u32, context| context.push(*value));
//! let mut context = Vec::new();
//! events.emit(1, 42_u32);
//! assert!(context.is_empty());
//! events.dispatch(&mut context);
//! assert_eq!(context, [42]);
//! drop(subscription);
//! ```

mod cursor;
mod dispatcher;
mod events;

pub use cursor::{EventCursor, EventIter};
pub use dispatcher::EventDispatcher;
pub use events::Events;
