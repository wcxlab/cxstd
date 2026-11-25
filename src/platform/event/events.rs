//! Shared platform lifecycle, gamepad connectivity and window input events.
//! Variants use Web event names where semantics match. Native window state,
//! pixel ratio, redraw and text input events are framework extensions.
//! Client positions, motion and resize sizes use logical pixels at the window's
//! current scale. Resize events also include physical sizes; wheel deltas retain native SDL units.
//! DOM targets and propagation belong to the view layer, not the native event stream.

use std::time::Duration;

use crate::{KeyCode, KeyLocation};

use crate::platform::window::WindowId;

/// Packed flags shared by keyboard, mouse and wheel event snapshots.
#[rustfmt::skip]
pub trait PlatformEventFlags {
    /// SDL modifiers in bits 0..15; keyboard repeat/composition in bits 16/17.
    /// Mouse buttons left/middle/right/back/forward occupy bits 18..22.
    fn flags(&self) -> u32;

    #[inline] fn ctrl_key(&self) -> bool { self.flags() & 0x00c0 != 0 }
    #[inline] fn shift_key(&self) -> bool { self.flags() & 0x0003 != 0 }
    #[inline] fn alt_key(&self) -> bool { self.flags() & 0x0300 != 0 }
    #[inline] fn meta_key(&self) -> bool { self.flags() & 0x0c00 != 0 }
    #[inline] fn caps_lock(&self) -> bool { self.flags() & 0x2000 != 0 }
    #[inline] fn num_lock(&self) -> bool { self.flags() & 0x1000 != 0 }
    #[inline] fn scroll_lock(&self) -> bool { self.flags() & 0x8000 != 0 }
    #[inline] fn alt_graph(&self) -> bool { self.flags() & 0x4000 != 0 }
    #[inline] fn repeat(&self) -> bool { self.flags() & (1 << 16) != 0 }
    #[inline] fn is_composing(&self) -> bool { self.flags() & (1 << 17) != 0 }

    #[inline] fn left_button(&self) -> bool { self.flags() & (1 << 18) != 0 }
    #[inline] fn middle_button(&self) -> bool { self.flags() & (1 << 19) != 0 }
    #[inline] fn right_button(&self) -> bool { self.flags() & (1 << 20) != 0 }
    #[inline] fn back_button(&self) -> bool { self.flags() & (1 << 21) != 0 }
    #[inline] fn forward_button(&self) -> bool { self.flags() & (1 << 22) != 0 }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlatformEventMessage {
    /// Elapsed time on the native event clock, preserving the SDL event timestamp.
    pub time_stamp: Duration,
    /// WindowId(0) identifies events without an associated window.
    pub window_id: WindowId,
    pub event: PlatformEvent,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlatformEvent {
    /// Request application exit or termination of the event polling loop.
    Quit,

    GamepadConnected(GamepadEvent),
    GamepadDisconnected(GamepadEvent),

    // Window events.
    Close,
    /// Request a frame after showing, exposure or an explicit redraw request.
    Redraw,
    Move(MoveEvent),
    /// Only logical size changes emit this event; duplicate native notifications are filtered.
    Resize(ResizeEvent),
    Minimize,
    Maximize,
    Restore,
    Focus,
    Blur,
    /// True on entry, false on exit.
    FullscreenChange(bool),
    DevicePixelRatioChange(DevicePixelRatioChangeEvent),

    // Mouse events.
    MouseEnter(MouseEvent),
    MouseLeave(MouseEvent),
    MouseMove(MouseEvent),
    MouseDown(MouseEvent),
    MouseUp(MouseEvent),
    Wheel(WheelEvent),

    // Keyboard and text input events.
    KeyDown(KeyboardEvent),
    KeyUp(KeyboardEvent),
    TextInput(TextInputEvent),
    /// Composition began with the first non-empty preedit; initial text is empty.
    CompositionStart(CompositionStartEvent),
    CompositionUpdate(CompositionUpdateEvent),
    /// Carries committed text after TextInput, or empty text on native clear/cancellation.
    CompositionEnd(CompositionEndEvent),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamepadEvent {
    /// Device metadata retained after disconnection; its index identifies a reusable slot.
    pub gamepad: crate::platform::gamepad::GamepadInfo,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoveEvent {
    pub screen_x: f32,
    pub screen_y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResizeEvent {
    pub inner_width: f32,
    pub inner_height: f32,
    pub inner_physical_width: u32,
    pub inner_physical_height: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DevicePixelRatioChangeEvent {
    pub device_pixel_ratio: f32,
    pub inner_physical_width: u32,
    pub inner_physical_height: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MouseEvent {
    pub screen_x: f64,
    pub screen_y: f64,
    pub client_x: f64,
    pub client_y: f64,
    pub movement_x: f64,
    pub movement_y: f64,
    /// Changed button for down/up; otherwise MouseLeft is the Web default button=0.
    /// Use the flags queries to determine which buttons are held.
    pub button: KeyCode,
    /// Click count for down/up; zero for movement and boundary events.
    pub detail: i32,

    pub(crate) flags: u32,
}

impl Default for MouseEvent {
    fn default() -> Self {
        Self {
            screen_x: 0.0,
            screen_y: 0.0,
            client_x: 0.0,
            client_y: 0.0,
            movement_x: 0.0,
            movement_y: 0.0,
            button: KeyCode::MouseLeft,
            detail: 0,
            flags: 0,
        }
    }
}

#[rustfmt::skip]
impl PlatformEventFlags for MouseEvent { #[inline] fn flags(&self) -> u32 { self.flags } }

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WheelEvent {
    pub screen_x: f64,
    pub screen_y: f64,
    pub client_x: f64,
    pub client_y: f64,
    /// Native SDL scroll amount, with positive Y scrolling down.
    pub delta_x: f64,
    pub delta_y: f64,
    pub delta_z: f64,

    pub(crate) flags: u32,
}

#[rustfmt::skip]
impl PlatformEventFlags for WheelEvent { #[inline] fn flags(&self) -> u32 { self.flags } }

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyboardEvent {
    pub key: String,
    pub code: KeyCode,
    pub location: KeyLocation,

    pub(crate) flags: u32,
}

#[rustfmt::skip]
impl PlatformEventFlags for KeyboardEvent { #[inline] fn flags(&self) -> u32 { self.flags } }

/// Committed text for a window. Preedit changes use CompositionUpdate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextInputEvent {
    pub data: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompositionStartEvent {
    pub data: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompositionUpdateEvent {
    pub data: String,
    /// Native preedit selection extension, expressed as UTF-16 offsets into data.
    pub selection_start: Option<u32>,
    pub selection_end: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompositionEndEvent {
    pub data: String,
}
