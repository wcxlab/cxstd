//! Private SDL device ownership and standard-layout state normalization.

use std::{
    cell::{Cell, RefCell},
    ffi::CStr,
    ptr::NonNull,
    rc::Rc,
    time::Duration,
};

use crate::GamepadButtonCode as ButtonCode;
use crate::platform::gamepad::{GamepadButton, GamepadInfo, GamepadState, GamepadType};

pub(crate) struct Gamepads {
    slots: Vec<Option<Device>>,
}

impl Gamepads {
    pub fn new() -> anyhow::Result<Self> {
        if !unsafe { sdl3_sys::init::SDL_InitSubSystem(sdl3_sys::init::SDL_INIT_GAMEPAD) } {
            anyhow::bail!(sdl_error());
        }
        let mut devices = Self { slots: Vec::new() };
        let mut count = 0;
        let ids = unsafe { sdl3_sys::gamepad::SDL_GetGamepads(&mut count) };
        if ids.is_null() {
            anyhow::bail!(sdl_error());
        }
        let ids_copy = unsafe { std::slice::from_raw_parts(ids, count as usize) }.to_vec();
        unsafe { sdl3_sys::stdinc::SDL_free(ids.cast()) };
        let timestamp = Duration::from_nanos(unsafe { sdl3_sys::timer::SDL_GetTicksNS() });
        for id in ids_copy {
            devices.add(id, timestamp)?;
        }
        Ok(devices)
    }

    pub fn states(&self) -> Vec<Option<Rc<GamepadState>>> {
        self.slots
            .iter()
            .map(|slot| slot.as_ref().map(|device| device.state.clone()))
            .collect()
    }

    pub fn state(&self, index: u32) -> Option<Rc<GamepadState>> {
        self.slots
            .get(index as usize)?
            .as_ref()
            .map(|device| device.state.clone())
    }

    /// Only connection changes produce public events; input updates the cache.
    pub fn handle_event(
        &mut self,
        event: &sdl3_sys::events::SDL_Event,
    ) -> Option<(bool, GamepadInfo)> {
        let kind = sdl3_sys::events::SDL_EventType(unsafe { event.r#type });
        let id = match kind {
            sdl3_sys::events::SDL_EVENT_GAMEPAD_ADDED
            | sdl3_sys::events::SDL_EVENT_GAMEPAD_REMOVED
            | sdl3_sys::events::SDL_EVENT_GAMEPAD_REMAPPED => unsafe { event.gdevice.which },
            sdl3_sys::events::SDL_EVENT_GAMEPAD_AXIS_MOTION => unsafe { event.gaxis.which },
            sdl3_sys::events::SDL_EVENT_GAMEPAD_BUTTON_DOWN
            | sdl3_sys::events::SDL_EVENT_GAMEPAD_BUTTON_UP => unsafe { event.gbutton.which },
            _ => return None,
        };
        let timestamp = Duration::from_nanos(unsafe { event.common.timestamp });
        if kind == sdl3_sys::events::SDL_EVENT_GAMEPAD_ADDED {
            return self
                .add(id, timestamp)
                .ok()
                .map(|state| (true, state.info.clone()));
        }
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|device| device.id == id))?;
        if kind == sdl3_sys::events::SDL_EVENT_GAMEPAD_REMOVED {
            let device = slot.take()?;
            device.state.connected.set(false);
            device.state.timestamp.set(timestamp);
            device.state.set_buttons([GamepadButton::default(); 17]);
            device.state.axes.set([0.0; 4]);
            return Some((false, device.state.info.clone()));
        }
        slot.as_ref()?.refresh(timestamp);
        None
    }

    fn add(
        &mut self,
        id: sdl3_sys::joystick::SDL_JoystickID,
        timestamp: Duration,
    ) -> anyhow::Result<Rc<GamepadState>> {
        if let Some(device) = self.slots.iter().flatten().find(|device| device.id == id) {
            return Ok(device.state.clone());
        }
        let raw = NonNull::new(unsafe { sdl3_sys::gamepad::SDL_OpenGamepad(id) })
            .ok_or_else(|| anyhow::anyhow!(sdl_error()))?;
        let name = unsafe { sdl3_sys::gamepad::SDL_GetGamepadName(raw.as_ptr()) };
        let name = if name.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned()
        };
        let index = self
            .slots
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.slots.len());
        let state = Rc::new(GamepadState {
            info: GamepadInfo {
                id: name,
                index: index as u32,
                vendor_id: unsafe { sdl3_sys::gamepad::SDL_GetGamepadVendorForID(id) },
                product_id: unsafe { sdl3_sys::gamepad::SDL_GetGamepadProductForID(id) },
                product_version: unsafe {
                    sdl3_sys::gamepad::SDL_GetGamepadProductVersionForID(id)
                },
                gamepad_type: match unsafe { sdl3_sys::gamepad::SDL_GetRealGamepadTypeForID(id) } {
                    sdl3_sys::gamepad::SDL_GamepadType::STANDARD => GamepadType::Standard,
                    sdl3_sys::gamepad::SDL_GamepadType::XBOX360 => GamepadType::Xbox360,
                    sdl3_sys::gamepad::SDL_GamepadType::XBOXONE => GamepadType::XboxOne,
                    sdl3_sys::gamepad::SDL_GamepadType::PS3 => GamepadType::PlayStation3,
                    sdl3_sys::gamepad::SDL_GamepadType::PS4 => GamepadType::PlayStation4,
                    sdl3_sys::gamepad::SDL_GamepadType::PS5 => GamepadType::PlayStation5,
                    sdl3_sys::gamepad::SDL_GamepadType::NINTENDO_SWITCH_PRO => {
                        GamepadType::NintendoSwitchPro
                    }
                    sdl3_sys::gamepad::SDL_GamepadType::NINTENDO_SWITCH_JOYCON_LEFT => {
                        GamepadType::NintendoSwitchJoyConLeft
                    }
                    sdl3_sys::gamepad::SDL_GamepadType::NINTENDO_SWITCH_JOYCON_RIGHT => {
                        GamepadType::NintendoSwitchJoyConRight
                    }
                    sdl3_sys::gamepad::SDL_GamepadType::NINTENDO_SWITCH_JOYCON_PAIR => {
                        GamepadType::NintendoSwitchJoyConPair
                    }
                    sdl3_sys::gamepad::SDL_GamepadType::GAMECUBE => GamepadType::GameCube,
                    sdl3_sys::gamepad::SDL_GamepadType::STEAM => GamepadType::Steam,
                    _ => GamepadType::Unknown,
                },
            },
            connected: Cell::new(true),
            timestamp: Cell::new(timestamp),
            buttons: RefCell::new(Rc::new([GamepadButton::default(); 17])),
            axes: Cell::new([0.0; 4]),
        });
        let device = Device {
            id,
            raw,
            state: state.clone(),
        };
        device.refresh(timestamp);
        if index == self.slots.len() {
            self.slots.push(Some(device));
        } else {
            self.slots[index] = Some(device);
        }
        Ok(state)
    }
}

impl Drop for Gamepads {
    fn drop(&mut self) {
        self.slots.clear();
        unsafe { sdl3_sys::init::SDL_QuitSubSystem(sdl3_sys::init::SDL_INIT_GAMEPAD) };
    }
}

struct Device {
    id: sdl3_sys::joystick::SDL_JoystickID,
    raw: NonNull<sdl3_sys::gamepad::SDL_Gamepad>,
    state: Rc<GamepadState>,
}

impl Device {
    fn refresh(&self, timestamp: Duration) {
        let axis = |axis| unsafe { sdl3_sys::gamepad::SDL_GetGamepadAxis(self.raw.as_ptr(), axis) };
        self.state.axes.set(
            [
                sdl3_sys::gamepad::SDL_GamepadAxis::LEFTX,
                sdl3_sys::gamepad::SDL_GamepadAxis::LEFTY,
                sdl3_sys::gamepad::SDL_GamepadAxis::RIGHTX,
                sdl3_sys::gamepad::SDL_GamepadAxis::RIGHTY,
            ]
            .map(|kind| {
                let value = axis(kind);
                f32::from(value) / if value < 0 { 32768.0 } else { 32767.0 }
            }),
        );
        let mut buttons = [GamepadButton::default(); 17];
        for (code, kind) in [
            (
                ButtonCode::South,
                sdl3_sys::gamepad::SDL_GamepadButton::SOUTH,
            ),
            (ButtonCode::East, sdl3_sys::gamepad::SDL_GamepadButton::EAST),
            (ButtonCode::West, sdl3_sys::gamepad::SDL_GamepadButton::WEST),
            (
                ButtonCode::North,
                sdl3_sys::gamepad::SDL_GamepadButton::NORTH,
            ),
            (
                ButtonCode::LeftShoulder,
                sdl3_sys::gamepad::SDL_GamepadButton::LEFT_SHOULDER,
            ),
            (
                ButtonCode::RightShoulder,
                sdl3_sys::gamepad::SDL_GamepadButton::RIGHT_SHOULDER,
            ),
            (
                ButtonCode::CenterLeft,
                sdl3_sys::gamepad::SDL_GamepadButton::BACK,
            ),
            (
                ButtonCode::CenterRight,
                sdl3_sys::gamepad::SDL_GamepadButton::START,
            ),
            (
                ButtonCode::LeftStick,
                sdl3_sys::gamepad::SDL_GamepadButton::LEFT_STICK,
            ),
            (
                ButtonCode::RightStick,
                sdl3_sys::gamepad::SDL_GamepadButton::RIGHT_STICK,
            ),
            (
                ButtonCode::DPadUp,
                sdl3_sys::gamepad::SDL_GamepadButton::DPAD_UP,
            ),
            (
                ButtonCode::DPadDown,
                sdl3_sys::gamepad::SDL_GamepadButton::DPAD_DOWN,
            ),
            (
                ButtonCode::DPadLeft,
                sdl3_sys::gamepad::SDL_GamepadButton::DPAD_LEFT,
            ),
            (
                ButtonCode::DPadRight,
                sdl3_sys::gamepad::SDL_GamepadButton::DPAD_RIGHT,
            ),
            (
                ButtonCode::Home,
                sdl3_sys::gamepad::SDL_GamepadButton::GUIDE,
            ),
        ] {
            let pressed =
                unsafe { sdl3_sys::gamepad::SDL_GetGamepadButton(self.raw.as_ptr(), kind) };
            buttons[code as usize] = GamepadButton {
                pressed,
                touched: pressed,
                value: if pressed { 1.0 } else { 0.0 },
            };
        }
        for (code, kind) in [
            (
                ButtonCode::LeftTrigger,
                sdl3_sys::gamepad::SDL_GamepadAxis::LEFT_TRIGGER,
            ),
            (
                ButtonCode::RightTrigger,
                sdl3_sys::gamepad::SDL_GamepadAxis::RIGHT_TRIGGER,
            ),
        ] {
            let value = f32::from(axis(kind).max(0)) / 32767.0;
            // Without touch sensors, nonzero travel means touched; half travel means pressed.
            buttons[code as usize] = GamepadButton {
                pressed: value > 0.5,
                touched: value > 0.0,
                value,
            };
        }
        self.state.set_buttons(buttons);
        self.state.timestamp.set(timestamp);
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { sdl3_sys::gamepad::SDL_CloseGamepad(self.raw.as_ptr()) };
    }
}

fn sdl_error() -> String {
    unsafe { CStr::from_ptr(sdl3_sys::error::SDL_GetError()) }
        .to_string_lossy()
        .into_owned()
}
