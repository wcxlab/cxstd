use super::{WindowDisplayId, WindowId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowEventCode(pub u32);

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
pub struct WindowCommonEvent {
    pub event_type: u32,
    pub reserved: u32,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowDisplayEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub display_id: WindowDisplayId,
    pub data1: i32,
    pub data2: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowKeyboardDeviceEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowKeyboardId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowKeyboardEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
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
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
    pub text: String,
    pub start: i32,
    pub length: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowTextEditingCandidatesEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
    pub candidates: Vec<String>,
    pub num_candidates: i32,
    pub selected_candidate: i32,
    pub horizontal: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowTextInputEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowMouseDeviceEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowMouseId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowMouseMotionEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
    pub which: WindowMouseId,
    pub state: WindowMouseButtonFlags,
    pub x: f32,
    pub y: f32,
    pub xrel: f32,
    pub yrel: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowMouseButtonEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
    pub which: WindowMouseId,
    pub button: u8,
    pub down: bool,
    pub clicks: u8,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowMouseWheelEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
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
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowJoystickId,
    pub axis: u8,
    pub value: i16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowJoyBallEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowJoystickId,
    pub ball: u8,
    pub xrel: i16,
    pub yrel: i16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowJoyHatEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowJoystickId,
    pub hat: u8,
    pub value: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowJoyButtonEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowJoystickId,
    pub button: u8,
    pub down: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowJoyDeviceEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowJoystickId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowJoyBatteryEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowJoystickId,
    pub state: WindowPowerState,
    pub percent: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowGamepadAxisEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowJoystickId,
    pub axis: u8,
    pub value: i16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowGamepadButtonEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowJoystickId,
    pub button: u8,
    pub down: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowGamepadDeviceEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowJoystickId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowGamepadTouchpadEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowJoystickId,
    pub touchpad: i32,
    pub finger: i32,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowGamepadSensorEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowJoystickId,
    pub sensor: i32,
    pub data: [f32; 3],
    pub sensor_timestamp: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowAudioDeviceEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowAudioDeviceId,
    pub recording: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowCameraDeviceEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowCameraId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowRenderEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowTouchFingerEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub touch_id: WindowTouchId,
    pub finger_id: WindowFingerId,
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub pressure: f32,
    pub window_id: WindowId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPinchFingerEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub scale: f32,
    pub window_id: WindowId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPenProximityEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
    pub which: WindowPenId,
    pub pen_state: WindowPenInputFlags,
    pub device_type: WindowPenDeviceType,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPenMotionEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
    pub which: WindowPenId,
    pub pen_state: WindowPenInputFlags,
    pub x: f32,
    pub y: f32,
    pub device_type: WindowPenDeviceType,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPenTouchEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
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
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
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
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
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
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
    pub x: f32,
    pub y: f32,
    pub source: Option<String>,
    pub data: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowClipboardEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub owner: bool,
    pub num_mime_types: i32,
    pub mime_types: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowSensorEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
    pub which: WindowSensorId,
    pub data: [f32; 6],
    pub sensor_timestamp: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowQuitEvent {
    pub event_type: WindowEventCode,
    pub reserved: u32,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowUserEvent {
    pub event_type: u32,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: WindowId,
    pub code: i32,
    pub data1: *mut std::ffi::c_void,
    pub data2: *mut std::ffi::c_void,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    Quit(WindowQuitEvent),
    Terminating(WindowCommonEvent),
    LowMemory(WindowCommonEvent),
    WillEnterBackground(WindowCommonEvent),
    DidEnterBackground(WindowCommonEvent),
    WillEnterForeground(WindowCommonEvent),
    DidEnterForeground(WindowCommonEvent),
    LocaleChanged(WindowCommonEvent),
    SystemThemeChanged(WindowCommonEvent),
    DisplayOrientation(WindowDisplayEvent),
    DisplayAdded(WindowDisplayEvent),
    DisplayRemoved(WindowDisplayEvent),
    DisplayMoved(WindowDisplayEvent),
    DisplayDesktopModeChanged(WindowDisplayEvent),
    DisplayCurrentModeChanged(WindowDisplayEvent),
    DisplayContentScaleChanged(WindowDisplayEvent),
    DisplayUsableBoundsChanged(WindowDisplayEvent),
    WindowShown {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowHidden {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowExposed {
        timestamp: u64,
        window_id: WindowId,
        live_resize: bool,
    },
    WindowMoved {
        timestamp: u64,
        window_id: WindowId,
        x: i32,
        y: i32,
    },
    WindowResized {
        timestamp: u64,
        window_id: WindowId,
        width: i32,
        height: i32,
    },
    WindowPixelSizeChanged {
        timestamp: u64,
        window_id: WindowId,
        width: i32,
        height: i32,
    },
    WindowMetalViewResized {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowMinimized {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowMaximized {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowRestored {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowMouseEnter {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowMouseLeave {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowFocusGained {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowFocusLost {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowCloseRequested {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowHitTest {
        timestamp: u64,
        window_id: WindowId,
        data1: i32,
        data2: i32,
    },
    WindowICCProfChanged {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowDisplayChanged {
        timestamp: u64,
        window_id: WindowId,
        display_id: WindowDisplayId,
    },
    WindowDisplayScaleChanged {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowSafeAreaChanged {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowOccluded {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowEnterFullscreen {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowLeaveFullscreen {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowDestroyed {
        timestamp: u64,
        window_id: WindowId,
    },
    WindowHdrStateChanged {
        timestamp: u64,
        window_id: WindowId,
    },
    KeyDown(WindowKeyboardEvent),
    KeyUp(WindowKeyboardEvent),
    TextEditing(WindowTextEditingEvent),
    TextInput(WindowTextInputEvent),
    KeymapChanged(WindowCommonEvent),
    KeyboardAdded(WindowKeyboardDeviceEvent),
    KeyboardRemoved(WindowKeyboardDeviceEvent),
    TextEditingCandidates(WindowTextEditingCandidatesEvent),
    ScreenKeyboardShown(WindowCommonEvent),
    ScreenKeyboardHidden(WindowCommonEvent),
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
    RenderTargetsReset(WindowRenderEvent),
    RenderDeviceReset(WindowRenderEvent),
    RenderDeviceLost(WindowRenderEvent),
    Private0(WindowCommonEvent),
    Private1(WindowCommonEvent),
    Private2(WindowCommonEvent),
    Private3(WindowCommonEvent),
    PollSentinel(WindowCommonEvent),
    User(WindowUserEvent),
    Unknown(WindowCommonEvent),
}
