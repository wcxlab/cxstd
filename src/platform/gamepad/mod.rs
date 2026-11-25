//! Standard gamepads through `get_gamepads()` and cached `Gamepad` handles.
//! Call `cxstd::platform::poll_event()` to update state and receive connection events.
//! Slots retain their indices until disconnection; vacant slots can be reused.
//! Handles share live state. Button reads return a shared snapshot.
//! Connection events carry owned GamepadInfo data without retaining native handles.
//! Platform-owned monitoring follows its shared context's lifetime. The standalone
//! `get_gamepads()` entry point keeps monitoring until the polling thread exits.

mod service;

pub use service::GamepadService;

use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
    time::Duration,
};

use crate::GamepadAxis;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GamepadType {
    Unknown,
    Standard,
    Xbox360,
    XboxOne,
    PlayStation3,
    PlayStation4,
    PlayStation5,
    NintendoSwitchPro,
    NintendoSwitchJoyConLeft,
    NintendoSwitchJoyConRight,
    NintendoSwitchJoyConPair,
    GameCube,
    Steam,
}

/// Owned device metadata; contains no native handle or live input state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamepadInfo {
    /// Product description, matching Web Gamepad.id; not a unique instance ID.
    pub id: String,
    pub index: u32,

    /// Physical device type, ignoring mapping overrides.
    pub gamepad_type: GamepadType,
    /// Zero means unavailable for vendor, product and product version values.
    pub vendor_id: u16,
    pub product_id: u16,
    pub product_version: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GamepadButton {
    pub pressed: bool,
    pub touched: bool,
    pub value: f32,
}

/// A standard-layout device, kept alive independently of its window.
#[derive(Clone)]
pub struct Gamepad {
    state: Rc<GamepadState>,
    _context: Rc<crate::platform::backend::sdl::SdlContext>,
}

impl Gamepad {
    pub fn info(&self) -> &GamepadInfo {
        &self.state.info
    }

    /// Product description; identical devices may share this string.
    pub fn id(&self) -> &str {
        &self.state.info.id
    }

    pub fn index(&self) -> u32 {
        self.state.info.index
    }

    pub fn connected(&self) -> bool {
        self.state.connected.get()
    }

    pub fn timestamp(&self) -> Duration {
        self.state.timestamp.get()
    }

    /// Share the current button array in standard Web order.
    /// Snapshots remain valid after subsequent calls to platform polling.
    pub fn buttons(&self) -> Rc<[GamepadButton; 17]> {
        Rc::clone(&self.state.buttons.borrow())
    }

    /// Left X/Y followed by right X/Y, normalized to -1..=1.
    pub fn axes(&self) -> [f32; 4] {
        self.state.axes.get()
    }

    pub fn axis(&self, axis: GamepadAxis) -> Option<f32> {
        self.axes().get(axis as usize).copied()
    }
}

impl Gamepad {
    pub(crate) fn new(
        state: Rc<GamepadState>,
        context: Rc<crate::platform::backend::sdl::SdlContext>,
    ) -> Self {
        Self {
            state,
            _context: context,
        }
    }
}

impl fmt::Debug for Gamepad {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Gamepad")
            .field("id", &self.id())
            .field("index", &self.index())
            .field("connected", &self.connected())
            .finish()
    }
}

impl PartialEq for Gamepad {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.state, &other.state)
    }
}

/// Enumerate standard gamepads, initializing native support automatically.
/// Disconnected slots are None; reconnecting devices receive a new handle.
/// Call on the window thread. The native backend is never exposed.
pub fn get_gamepads() -> anyhow::Result<Vec<Option<Gamepad>>> {
    crate::platform::backend::sdl::get_gamepads()
}

pub(crate) struct GamepadState {
    pub info: GamepadInfo,
    pub connected: Cell<bool>,
    pub timestamp: Cell<Duration>,
    pub buttons: RefCell<Rc<[GamepadButton; 17]>>,
    pub axes: Cell<[f32; 4]>,
}

impl GamepadState {
    pub fn set_buttons(&self, buttons: [GamepadButton; 17]) {
        let mut snapshot = self.buttons.borrow_mut();
        *Rc::make_mut(&mut snapshot) = buttons;
    }
}
