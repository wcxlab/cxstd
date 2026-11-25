//! Serial ECS instances through `Ecs::new()`.
//!
//! Ecs owns its World, Resources, TaskExecutor, EventDispatcher, CommandQueue, and
//! named stages. Each stage owns a Schedule. World contains entity state and components only.
//! State, components, and resources need not be Send, Sync, or Clone.
//! Event buffers, dispatch, observers, and tasks live in this module and are
//! re-exported here, such as `cxstd::ecs::{Events, Subscription, TaskExecutor}`.
//! Native platform events are exported separately at the crate root.
//!
//! `ecs.add_system("update", system)` registers a system in the selected stage.
//! Register the stage with `ecs.add_stage("update")` first; adding a system to an
//! unknown stage returns an error without creating it.
//! `ecs.run_stage("update")` advances facilities and executes that stage's schedule.
//! The caller controls order. Stages are case-sensitive strings, local to each Ecs.
//! Registration accepts `&str` or String; execution borrows the stage for lookup.
//! `ecs.add_stage(stage)` registers an empty stage or establishes its registration order.
//! Executing an unknown stage returns an error.
//! `ecs.flush()` advances facilities alone; `ecs.run()` explicitly executes all
//! registered stages in registration order without advancing facilities.
//! Command barriers consume one batch and remain explicit.
//! ECS-owned Plugin instances use `ecs.add_plugin(plugin)` to obtain a PluginRef,
//! then `ecs.remove_plugin(handle)` for cleanup and removal. Deps declare
//! injected dependencies. Hooks receive PluginContext instead of the owning Ecs.
//! `Entity::new()` collects components through `entity.add_component(component)`;
//! `world.add_entity(entity)` transfers them into the World and returns an EntityId.
//!
//! Applications register dependencies with `ecs.add_resource(value)`. Systems
//! obtain queries and the executing Ecs's facilities through Dependences.
//! An optional host type C is supported by `run_with`/`run_stage_with` when needed.
//! `Res<T>`/`ResMut<T>` fetch this instance's resources. TaskEntity values added
//! to World own foreground work. Systems inject CommandQueue for deferred changes, or EventDispatcher
//! for targeted events. EcsContext aggregates these facilities internally and
//! is not a Dependences. Explicit command barriers apply deferred work.
//! Structs implement System and declare their dependency type through System::Deps.
//! System::run takes `(&mut self, ctx, deps)`: SystemContext first after self, then
//! one Dependences containing its dependencies. Use a tuple for multiple dependencies
//! or `()` for none. The context exposes resource access, read-only World access, and
//! deferred entity operations such as `ctx.add_entity(entity)`. The command queue is
//! hidden by the context. PluginContext additionally registers systems, stages,
//! and command barriers; SystemContext has no system-registration interface.
//! Resource registrations apply after the hook or schedule; World commands keep
//! explicit command barriers. Dynamic lookups use runtime borrow checks in this serial executor.
//!
//! `&World` conflicts with mutable queries; `&Resources` conflicts with ResMut.
//! Applications can implement additional Dependences adapters for their host.
//! Exclusive functions accept `&mut World`, `&mut C`, or both in either order.
//! An exclusive `&mut World` function may also declare one `Res<T>` parameter.
//! Conflicting dependencies fail before the system runs. Queries and dependencies
//! support tuples of up to eight items. Execution is serial.
//!
//! ```
//! use cxstd::ecs::Resource;
//! use cxstd::ecs::{CommandQueue, Component, Ecs, Entity, Query, Res, System, SystemContext};
//!
//! struct Position(f32);
//! impl Component for Position {}
//! struct Step(f32);
//! impl Resource for Step {}
//!
//! struct Setup;
//! impl System for Setup {
//!     type Deps = CommandQueue;
//!
//!     fn run(&mut self, _ctx: SystemContext, deps: CommandQueue) -> anyhow::Result<()> {
//!         let commands = deps;
//!         let mut entity = Entity::new();
//!         entity.add_component(Position(1.0));
//!         commands.add_entity(entity);
//!         Ok(())
//!     }
//! }
//!
//! struct Advance;
//! impl System for Advance {
//!     type Deps = (Query<'static, &'static mut Position>, Res<'static, Step>);
//!
//!     fn run(&mut self, _ctx: SystemContext, deps: (Query<&mut Position>, Res<Step>)) -> anyhow::Result<()> {
//!         let (mut positions, step) = deps;
//!         for mut position in positions.iter_mut() {
//!             position.0 += step.0;
//!         }
//!         Ok(())
//!     }
//! }
//!
//! # fn main() -> anyhow::Result<()> {
//! let runtime = tokio::runtime::Runtime::new()?;
//! let _entered = runtime.enter();
//! let mut ecs = Ecs::new()?;
//! ecs.add_resource(Step(0.5))?;
//! ecs.add_stage("setup");
//! ecs.add_stage("update");
//! ecs.add_system("setup", Setup)?;
//! ecs.add_apply_deferred("setup")?;
//! ecs.add_system("update", Advance)?;
//! ecs.run_stage("setup")?;
//! ecs.run_stage("update")?;
//! assert_eq!(ecs.world().query::<Position>().next().unwrap().1.0, 1.5);
//! # Ok(())
//! # }
//! ```

pub mod r#async;
mod command_queue;
mod component;
mod context;
mod ecs;
mod entity;
pub mod event;
pub mod observer;
mod param;
mod plugin;
mod query;
mod registration;
mod resource;
mod schedule;
mod scoped;
mod system;
mod world;

pub use r#async::{
    AsyncCallback, AsyncClosed, BackgroundExecutor, ForegroundExecutor, Task, TaskAbortHandle,
    TaskEntity, TaskExecutor, TaskId, TaskRef, TaskScope,
};
pub use command_queue::CommandQueue;
pub use component::{Component, ComponentMut, ComponentRef};
pub use context::{EcsContext, PluginContext, SystemContext};
pub use ecs::Ecs;
pub use entity::{Entity, EntityId, EntityRef, EntityStateMut, EntityStateRef};
pub use event::{EventCursor, EventDispatcher, EventIter, Events};
pub use observer::{ObserverId, ObserverRegistry, Observers, Subscription};
pub use param::{Dependences, SystemAccess, SystemInput};
pub use plugin::{Plugin, PluginRef};
pub use query::{Query, QueryData, ReadOnlyQueryData};
pub use resource::{Res, ResMut, Resource, ResourceScope, Resources};
pub use schedule::Schedule;
pub use scoped::{ScopedEcs, ScopedResources};
pub use system::{IntoSystem, System, SystemOutput};
pub use world::World;
