//! Application-owned plugins configured through `plugin.load(&mut ecs)` and
//! released through `plugin.unload(&mut ecs)` before the ECS is dropped.

use super::Ecs;
use crate::Result;

/// Groups ECS setup and optional cleanup behind an application-owned instance.
pub trait Plugin<C: 'static = ()> {
    fn load(&mut self, ecs: &mut Ecs<C>) -> Result<()>;

    /// Release plugin-owned handles. The default does nothing; ECS entities,
    /// resources and system registrations remain owned by the ECS.
    fn unload(&mut self, _ecs: &mut Ecs<C>) -> Result<()> {
        Ok(())
    }
}
