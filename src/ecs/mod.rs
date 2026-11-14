//! Serial ECS instances through `Ecs::new()`.
//!
//! Ecs owns its World, Resources, TaskExecutor, EventDispatcher, CommandQueue, and
//! named schedules. World contains entity state and components only.
//! State, components, and resources need not be Send, Sync, or Clone.
//! Event buffers, dispatch, observers, and tasks live in this module and are
//! re-exported here, such as `cxstd::ecs::{Events, Subscription, TaskExecutor}`.
//! Native platform events are defined separately in `cxstd::platform`.
//!
//! `ecs.add_system("update", system)` registers a system by schedule name.
//! `ecs.run_schedule("update")` advances facilities and executes only that schedule.
//! The caller controls order. Names are case-sensitive strings, local to each Ecs.
//! Registration accepts `&str` or String; execution borrows the name for lookup.
//! `ecs.add_schedule(name)` registers an empty schedule. Unknown names return an error.
//! `ecs.flush()` advances facilities alone; `ecs.run()` explicitly executes all
//! registered schedules in registration order without advancing facilities.
//! Command barriers consume one batch and remain explicit.
//! Application-owned Plugin instances expose load/unload hooks for setup and cleanup.
//! `Entity::new()` collects components through `entity.add_component(component)`;
//! `world.add_entity(entity)` transfers them into the World and returns an EntityId.
//!
//! Applications register dependencies with `ecs.add_resource(value)`. Systems
//! obtain queries and the executing Ecs's facilities through SystemParam.
//! An optional host type C is supported by `run_with`/`run_schedule_with` when needed.
//! `Res<T>`/`ResMut<T>` fetch this instance's resources. TaskEntity values added
//! to World own foreground work. Systems inject CommandQueue for deferred changes, or EventDispatcher
//! for targeted events. EcsContext aggregates these facilities internally and
//! is not a SystemParam. Explicit command barriers apply deferred work.
//! Custom System implementations receive the same borrowed EcsContext directly.
//!
//! `&World` conflicts with mutable queries; `&Resources` conflicts with ResMut.
//! Applications can implement additional SystemParam adapters for their host.
//! Exclusive functions accept `&mut World`, `&mut C`, or both in either order.
//! An exclusive `&mut World` function may also declare one `Res<T>` parameter.
//! Conflicting parameters fail before the function runs. Queries and functions
//! support tuples of up to eight items. Execution is serial and uses safe Rust.
//!
//! ```
//! use anyhow::Result;
//! use cxstd::Resource;
//! use cxstd::ecs::{CommandQueue, Component, Ecs, Entity, Query, Res};
//!
//! struct Position(f32);
//! impl Component for Position {}
//! struct Step(f32);
//! impl Resource for Step {}
//!
//! fn setup(commands: CommandQueue) {
//!     let mut entity = Entity::new();
//!     entity.add_component(Position(1.0));
//!     commands.add_entity(entity);
//! }
//!
//! fn advance(mut positions: Query<&mut Position>, step: Res<Step>) {
//!     for mut position in positions.iter_mut() {
//!         position.0 += step.0;
//!     }
//! }
//!
//! # fn main() -> Result<()> {
//! let runtime = tokio::runtime::Runtime::new()?;
//! let _entered = runtime.enter();
//! let mut ecs = Ecs::new()?;
//! ecs.add_resource(Step(0.5))?;
//! ecs.add_system("setup", setup);
//! ecs.add_apply_deferred("setup");
//! ecs.add_system("update", advance);
//! ecs.run_schedule("setup")?;
//! ecs.run_schedule("update")?;
//! assert_eq!(ecs.world().query::<Position>().next().unwrap().1.0, 1.5);
//! # Ok(())
//! # }
//! ```

pub mod r#async;
mod command_queue;
mod context;
mod ecs;
mod entity;
pub mod event;
mod function;
pub mod observer;
mod param;
mod plugin;
mod query;
mod resource;
mod schedule;
mod world;

pub use r#async::{
    AsyncCallback, AsyncClosed, BackgroundExecutor, ForegroundExecutor, Task, TaskAbortHandle,
    TaskEntity, TaskExecutor, TaskId, TaskScope,
};
pub use command_queue::CommandQueue;
pub use context::EcsContext;
pub use ecs::Ecs;
pub use entity::{Component, Entity, EntityId, EntityRef};
pub use event::{EventCursor, EventDispatcher, EventIter, Events};
pub use function::{IntoSystem, SystemOutput, SystemParamFunction};
pub use observer::{ObserverId, ObserverRegistry, Observers, Subscription};
pub use param::{SystemAccess, SystemInput, SystemParam};
pub use plugin::Plugin;
pub use query::{Query, QueryData, ReadOnlyQueryData};
pub use resource::{Res, ResMut, Resource, ResourceScope, Resources};
pub use schedule::{Schedule, System};
pub use world::World;
