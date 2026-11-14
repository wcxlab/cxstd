//! Standard gamepads through `get_gamepads()` and cached `Gamepad` handles.
//! Call `platform.event().poll_event()` to update state and receive connection events.
//! Slots retain their indices until disconnection; vacant slots can be reused.
//! Handles share live state, while button and axis queries return copies.
//! Connection events carry owned GamepadInfo data without retaining native handles.
//! Platform-owned monitoring follows its shared context's lifetime. The standalone
//! `get_gamepads()` entry point keeps monitoring until the polling thread exits.

mod subsystem;

pub use subsystem::GamepadSubsystem;

use std::{cell::Cell, fmt, rc::Rc, time::Duration};

use crate::{GamepadAxis, KeyCode, Result};

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

    /// Standard Web button order; trigger values retain their analog travel.
    pub fn buttons(&self) -> [GamepadButton; 17] {
        self.state.buttons.get()
    }

    /// Left X/Y followed by right X/Y, normalized to -1..=1.
    pub fn axes(&self) -> [f32; 4] {
        self.state.axes.get()
    }

    pub fn button(&self, code: KeyCode) -> Option<GamepadButton> {
        let index = match code {
            KeyCode::GamepadSouth => 0,
            KeyCode::GamepadEast => 1,
            KeyCode::GamepadWest => 2,
            KeyCode::GamepadNorth => 3,
            KeyCode::GamepadLeftShoulder => 4,
            KeyCode::GamepadRightShoulder => 5,
            KeyCode::GamepadLeftTrigger => 6,
            KeyCode::GamepadRightTrigger => 7,
            KeyCode::GamepadCenterLeft => 8,
            KeyCode::GamepadCenterRight => 9,
            KeyCode::GamepadLeftStick => 10,
            KeyCode::GamepadRightStick => 11,
            KeyCode::GamepadDPadUp => 12,
            KeyCode::GamepadDPadDown => 13,
            KeyCode::GamepadDPadLeft => 14,
            KeyCode::GamepadDPadRight => 15,
            KeyCode::GamepadHome => 16,
            _ => return None,
        };
        Some(self.state.buttons.get()[index])
    }

    /// Trigger axis extensions return the corresponding button's 0..=1 value.
    pub fn axis(&self, axis: GamepadAxis) -> Option<f32> {
        let axes = self.state.axes.get();
        Some(match axis {
            GamepadAxis::LeftStickX => axes[0],
            GamepadAxis::LeftStickY => axes[1],
            GamepadAxis::RightStickX => axes[2],
            GamepadAxis::RightStickY => axes[3],
            GamepadAxis::LeftTrigger => self.state.buttons.get()[6].value,
            GamepadAxis::RightTrigger => self.state.buttons.get()[7].value,
        })
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
pub fn get_gamepads() -> Result<Vec<Option<Gamepad>>> {
    crate::platform::backend::sdl::get_gamepads()
}

pub(crate) struct GamepadState {
    pub info: GamepadInfo,
    pub connected: Cell<bool>,
    pub timestamp: Cell<Duration>,
    pub buttons: Cell<[GamepadButton; 17]>,
    pub axes: Cell<[f32; 4]>,
}
