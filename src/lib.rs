mod cxstd;

pub use cxstd::{GamepadAxis, KeyCode, KeyLocation, Ref, RefMut, Result};
pub use ecs::{Res, ResMut, Resource, ResourceScope, Resources};
pub use platform::{PlatformEvent, PlatformEventFlags, PlatformEventMessage, gamepad, window};

pub mod ecs;
pub mod platform;
