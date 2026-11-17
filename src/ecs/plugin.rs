//! ECS-owned plugins registered through `ecs.add_plugin(plugin)`.
//! PluginContext is always the first hook argument after self, followed by deps.
//! Load dependencies use the same extraction as systems. Setup registrations are deferred.
//!
//! ```
//! use cxstd::ecs::{PluginContext, Ecs, Plugin, Res, ResMut, Resource, System, SystemContext};
//!
//! struct Initial(u32);
//! impl Resource for Initial {}
//! struct Count(u32);
//! impl Resource for Count {}
//! struct CounterPlugin;
//! struct CounterSystem;
//!
//! impl System for CounterSystem {
//!     type Deps = ResMut<'static, Count>;
//!
//!     fn run(&mut self, _ctx: SystemContext, deps: ResMut<Count>) -> anyhow::Result<()> {
//!         let mut count = deps;
//!         count.0 += 1;
//!         Ok(())
//!     }
//! }
//!
//! impl Plugin for CounterPlugin {
//!     type Deps = Res<'static, Initial>;
//!
//!     fn load(&mut self, ctx: PluginContext, deps: Res<Initial>) -> anyhow::Result<()> {
//!         let initial = deps;
//!         ctx.add_resource(Count(initial.0));
//!         ctx.add_system("update", CounterSystem);
//!         Ok(())
//!     }
//! }
//!
//! # fn main() -> anyhow::Result<()> {
//! let runtime = tokio::runtime::Runtime::new()?;
//! let _entered = runtime.enter();
//! let mut ecs = Ecs::new()?;
//! ecs.add_resource(Initial(2))?;
//! let plugin = ecs.add_plugin(CounterPlugin)?;
//! ecs.run_stage("update")?;
//! assert_eq!(ecs.resource::<Count>()?.0, 3);
//! ecs.remove_plugin(plugin)?;
//! # Ok(())
//! # }
//! ```

use std::{
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};

use super::{Dependences, Ecs, PluginContext};

/// Groups ECS setup and optional cleanup behind an ECS-owned instance.
pub trait Plugin<C: 'static = ()>: 'static {
    type Deps: Dependences<C>;

    /// Defaults to the Rust type name, including its module path.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    fn load(
        &mut self,
        ctx: PluginContext<'_, C>,
        deps: <Self::Deps as Dependences<C>>::Item<'_>,
    ) -> anyhow::Result<()>;

    /// Release plugin-owned handles. The default does nothing; ECS entities,
    /// resources and system registrations remain owned by the ECS.
    fn unload(&mut self, _ctx: PluginContext<'_, C>) -> anyhow::Result<()> {
        Ok(())
    }
}

static NEXT_PLUGIN: AtomicU64 = AtomicU64::new(1);

/// A typed identity for an ECS-owned plugin, without borrowing or owning it.
/// Removal or ECS destruction invalidates every copy. Dropping a handle does not
/// unload the plugin; call Ecs::remove_plugin to run its unload hook.
pub struct PluginRef<P> {
    pub(super) id: u64,
    marker: PhantomData<fn() -> P>,
}

impl<P> PluginRef<P> {
    pub(super) fn new() -> Self {
        Self {
            id: NEXT_PLUGIN.fetch_add(1, Ordering::Relaxed),
            marker: PhantomData,
        }
    }

    pub fn read<C: 'static>(self, ecs: &Ecs<C>) -> anyhow::Result<&P>
    where
        P: Plugin<C>,
    {
        ecs.plugin(self)
    }

    pub fn read_mut<C: 'static>(self, ecs: &mut Ecs<C>) -> anyhow::Result<&mut P>
    where
        P: Plugin<C>,
    {
        ecs.plugin_mut(self)
    }
}

impl<P> Copy for PluginRef<P> {}
impl<P> Clone for PluginRef<P> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<P> std::fmt::Debug for PluginRef<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PluginRef").field(&self.id).finish()
    }
}
impl<P> PartialEq for PluginRef<P> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl<P> Eq for PluginRef<P> {}
