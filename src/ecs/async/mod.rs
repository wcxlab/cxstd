//! Host-driven asynchronous work through `TaskExecutor::new()` and `update()`.
//!
//! The host provides a Tokio runtime and periodically calls `update()`. Send
//! futures run in the background; `update()` delivers results on the owning
//! thread, and polls runnable non-Send foreground futures. It never borrows ECS
//! state on a worker thread. A completion callback can enqueue `crate::ecs::CommandQueue`
//! for an explicit schedule barrier.
//! The `crate::ecs::Ecs` instance owns the executor; it can also be driven independently.
//!
//! `TaskScope` cancels related work. Awaitable tasks cancel on drop unless
//! detached; callback-driven tasks remain owned by the executor until completion,
//! explicit cancellation, or `close()`. Call `close()` before releasing the owner.

mod executor;
mod executors;
mod handle;
mod task_entity;

pub use executor::{AsyncCallback, TaskExecutor};
pub use executors::{BackgroundExecutor, ForegroundExecutor};
pub use handle::{AsyncClosed, Task, TaskAbortHandle, TaskId, TaskScope};
pub use task_entity::TaskEntity;
pub(crate) use task_entity::start_tasks;
