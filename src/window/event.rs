use super::{WindowDisplayId, WindowDisplayOrientation, WindowId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowKeyboardId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowMouseId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowJoystickId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowAudioDeviceId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowCameraId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowSensorId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowTouchId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowFingerId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowPenId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowScancode(pub i32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowKeycode(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowKeymod(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowMouseButtonFlags(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowMouseWheelDirection(pub i32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowPowerState(pub i32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowPenInputFlags(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowPenDeviceType(pub i32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowPenAxis(pub i32);

#[derive(Clone, Debug, PartialEq)]
pub struct WindowKeyboardDeviceEvent {
    pub which: WindowKeyboardId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowKeyboardEvent {
    pub which: WindowKeyboardId,
    pub scancode: WindowScancode,
    pub key: WindowKeycode,
    pub modifiers: WindowKeymod,
    pub raw: u16,
    pub down: bool,
    pub repeat: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowTextEditingEvent {
    pub text: String,
    pub start: i32,
    pub length: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowTextEditingCandidatesEvent {
    pub candidates: Vec<String>,
    pub num_candidates: i32,
    pub selected_candidate: i32,
    pub horizontal: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowTextInputEvent {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowMouseDeviceEvent {
    pub which: WindowMouseId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowMouseMotionEvent {
    pub which: WindowMouseId,
    pub state: WindowMouseButtonFlags,
    pub x: f32,
    pub y: f32,
    pub xrel: f32,
    pub yrel: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowMouseButtonEvent {
    pub which: WindowMouseId,
    pub button: u8,
    pub down: bool,
    pub clicks: u8,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowMouseWheelEvent {
    pub which: WindowMouseId,
    pub x: f32,
    pub y: f32,
    pub direction: WindowMouseWheelDirection,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub integer_x: i32,
    pub integer_y: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowJoyAxisEvent {
    pub which: WindowJoystickId,
    pub axis: u8,
    pub value: i16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowJoyBallEvent {
    pub which: WindowJoystickId,
    pub ball: u8,
    pub xrel: i16,
    pub yrel: i16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowJoyHatEvent {
    pub which: WindowJoystickId,
    pub hat: u8,
    pub value: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowJoyButtonEvent {
    pub which: WindowJoystickId,
    pub button: u8,
    pub down: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowJoyDeviceEvent {
    pub which: WindowJoystickId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowJoyBatteryEvent {
    pub which: WindowJoystickId,
    pub state: WindowPowerState,
    pub percent: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowGamepadAxisEvent {
    pub which: WindowJoystickId,
    pub axis: u8,
    pub value: i16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowGamepadButtonEvent {
    pub which: WindowJoystickId,
    pub button: u8,
    pub down: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowGamepadDeviceEvent {
    pub which: WindowJoystickId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowGamepadTouchpadEvent {
    pub which: WindowJoystickId,
    pub touchpad: i32,
    pub finger: i32,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowGamepadSensorEvent {
    pub which: WindowJoystickId,
    pub sensor: i32,
    pub data: [f32; 3],
    pub sensor_timestamp: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowAudioDeviceEvent {
    pub which: WindowAudioDeviceId,
    pub recording: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowCameraDeviceEvent {
    pub which: WindowCameraId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowTouchFingerEvent {
    pub touch_id: WindowTouchId,
    pub finger_id: WindowFingerId,
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub pressure: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPinchFingerEvent {
    pub scale: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPenProximityEvent {
    pub which: WindowPenId,
    pub pen_state: WindowPenInputFlags,
    pub device_type: WindowPenDeviceType,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPenMotionEvent {
    pub which: WindowPenId,
    pub pen_state: WindowPenInputFlags,
    pub x: f32,
    pub y: f32,
    pub device_type: WindowPenDeviceType,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPenTouchEvent {
    pub which: WindowPenId,
    pub pen_state: WindowPenInputFlags,
    pub x: f32,
    pub y: f32,
    pub eraser: bool,
    pub down: bool,
    pub device_type: WindowPenDeviceType,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPenButtonEvent {
    pub which: WindowPenId,
    pub pen_state: WindowPenInputFlags,
    pub x: f32,
    pub y: f32,
    pub button: u8,
    pub down: bool,
    pub device_type: WindowPenDeviceType,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPenAxisEvent {
    pub which: WindowPenId,
    pub pen_state: WindowPenInputFlags,
    pub x: f32,
    pub y: f32,
    pub axis: WindowPenAxis,
    pub value: f32,
    pub device_type: WindowPenDeviceType,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowDropEvent {
    pub x: f32,
    pub y: f32,
    pub source: Option<String>,
    pub data: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowClipboardEvent {
    pub owner: bool,
    pub num_mime_types: i32,
    pub mime_types: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowSensorEvent {
    pub which: WindowSensorId,
    pub data: [f32; 6],
    pub sensor_timestamp: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowEvent {
    pub timestamp: u64,
    /// WindowId(0) identifies events without an associated window.
    pub window_id: WindowId,
    pub kind: WindowEventKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WindowEventKind {
    Quit,
    Terminating,
    LowMemory,
    WillEnterBackground,
    DidEnterBackground,
    WillEnterForeground,
    DidEnterForeground,
    LocaleChanged,
    SystemThemeChanged,
    DisplayOrientation {
        display_id: WindowDisplayId,
        orientation: WindowDisplayOrientation,
    },
    DisplayAdded {
        display_id: WindowDisplayId,
    },
    DisplayRemoved {
        display_id: WindowDisplayId,
    },
    DisplayMoved {
        display_id: WindowDisplayId,
    },
    DisplayDesktopModeChanged {
        display_id: WindowDisplayId,
        width: i32,
        height: i32,
    },
    DisplayCurrentModeChanged {
        display_id: WindowDisplayId,
        width: i32,
        height: i32,
    },
    DisplayContentScaleChanged {
        display_id: WindowDisplayId,
    },
    DisplayUsableBoundsChanged {
        display_id: WindowDisplayId,
    },
    WindowShown,
    WindowHidden,
    WindowExposed {
        live_resize: bool,
    },
    WindowMoved {
        x: i32,
        y: i32,
    },
    WindowResized {
        width: i32,
        height: i32,
    },
    WindowPixelSizeChanged {
        width: i32,
        height: i32,
    },
    WindowMetalViewResized,
    WindowMinimized,
    WindowMaximized,
    WindowRestored,
    WindowMouseEnter,
    WindowMouseLeave,
    WindowFocusGained,
    WindowFocusLost,
    WindowCloseRequested,
    WindowHitTest {
        data1: i32,
        data2: i32,
    },
    WindowICCProfChanged,
    WindowDisplayChanged {
        display_id: WindowDisplayId,
    },
    WindowDisplayScaleChanged,
    WindowSafeAreaChanged,
    WindowOccluded,
    WindowEnterFullscreen,
    WindowLeaveFullscreen,
    WindowDestroyed,
    WindowHdrStateChanged,
    KeyDown(WindowKeyboardEvent),
    KeyUp(WindowKeyboardEvent),
    TextEditing(WindowTextEditingEvent),
    TextInput(WindowTextInputEvent),
    KeymapChanged,
    KeyboardAdded(WindowKeyboardDeviceEvent),
    KeyboardRemoved(WindowKeyboardDeviceEvent),
    TextEditingCandidates(WindowTextEditingCandidatesEvent),
    ScreenKeyboardShown,
    ScreenKeyboardHidden,
    MouseMotion(WindowMouseMotionEvent),
    MouseButtonDown(WindowMouseButtonEvent),
    MouseButtonUp(WindowMouseButtonEvent),
    MouseWheel(WindowMouseWheelEvent),
    MouseAdded(WindowMouseDeviceEvent),
    MouseRemoved(WindowMouseDeviceEvent),
    JoystickAxisMotion(WindowJoyAxisEvent),
    JoystickBallMotion(WindowJoyBallEvent),
    JoystickHatMotion(WindowJoyHatEvent),
    JoystickButtonDown(WindowJoyButtonEvent),
    JoystickButtonUp(WindowJoyButtonEvent),
    JoystickAdded(WindowJoyDeviceEvent),
    JoystickRemoved(WindowJoyDeviceEvent),
    JoystickBatteryUpdated(WindowJoyBatteryEvent),
    JoystickUpdateComplete(WindowJoyDeviceEvent),
    GamepadAxisMotion(WindowGamepadAxisEvent),
    GamepadButtonDown(WindowGamepadButtonEvent),
    GamepadButtonUp(WindowGamepadButtonEvent),
    GamepadAdded(WindowGamepadDeviceEvent),
    GamepadRemoved(WindowGamepadDeviceEvent),
    GamepadRemapped(WindowGamepadDeviceEvent),
    GamepadTouchpadDown(WindowGamepadTouchpadEvent),
    GamepadTouchpadMotion(WindowGamepadTouchpadEvent),
    GamepadTouchpadUp(WindowGamepadTouchpadEvent),
    GamepadSensorUpdate(WindowGamepadSensorEvent),
    GamepadUpdateComplete(WindowGamepadDeviceEvent),
    GamepadSteamHandleUpdated(WindowGamepadDeviceEvent),
    FingerDown(WindowTouchFingerEvent),
    FingerUp(WindowTouchFingerEvent),
    FingerMotion(WindowTouchFingerEvent),
    FingerCanceled(WindowTouchFingerEvent),
    PinchBegin(WindowPinchFingerEvent),
    PinchUpdate(WindowPinchFingerEvent),
    PinchEnd(WindowPinchFingerEvent),
    ClipboardUpdate(WindowClipboardEvent),
    DropFile(WindowDropEvent),
    DropText(WindowDropEvent),
    DropBegin(WindowDropEvent),
    DropComplete(WindowDropEvent),
    DropPosition(WindowDropEvent),
    AudioDeviceAdded(WindowAudioDeviceEvent),
    AudioDeviceRemoved(WindowAudioDeviceEvent),
    AudioDeviceFormatChanged(WindowAudioDeviceEvent),
    SensorUpdate(WindowSensorEvent),
    PenProximityIn(WindowPenProximityEvent),
    PenProximityOut(WindowPenProximityEvent),
    PenDown(WindowPenTouchEvent),
    PenUp(WindowPenTouchEvent),
    PenButtonDown(WindowPenButtonEvent),
    PenButtonUp(WindowPenButtonEvent),
    PenMotion(WindowPenMotionEvent),
    PenAxis(WindowPenAxisEvent),
    CameraDeviceAdded(WindowCameraDeviceEvent),
    CameraDeviceRemoved(WindowCameraDeviceEvent),
    CameraDeviceApproved(WindowCameraDeviceEvent),
    CameraDeviceDenied(WindowCameraDeviceEvent),
    RenderTargetsReset,
    RenderDeviceReset,
    RenderDeviceLost,
}
