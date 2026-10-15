use std::{
    ffi::{CStr, CString, c_char},
    marker::PhantomData,
    mem::MaybeUninit,
    ptr::NonNull,
    rc::Rc,
    sync::Mutex,
    thread::{self, ThreadId},
};

use sdl3_sys as sys;

use super::super::*;

static SDL_CONTEXTS: Mutex<Option<(ThreadId, usize)>> = Mutex::new(None);

struct SdlContext;

pub struct SdlInstance {
    context: Rc<SdlContext>,
}

pub struct WindowInstance {
    raw: NonNull<sys::video::SDL_Window>,
    _context: Rc<SdlContext>,
}

pub fn init() -> Result<SdlInstance, WindowError> {
    let mut contexts = SDL_CONTEXTS.lock().map_err(|_| WindowError::Unknown)?;
    let current_thread = thread::current().id();
    if let Some((owner, _)) = contexts.as_ref() {
        if *owner != current_thread {
            return Err(WindowError::Nul(
                "window context must be initialized on the video thread".into(),
            ));
        }
    } else {
        #[cfg(target_vendor = "apple")]
        if !unsafe { sys::init::SDL_IsMainThread() } {
            return Err(WindowError::Nul(
                "window context must be initialized on the main thread".into(),
            ));
        }
        unsafe { sys::main::SDL_SetMainReady() };
    }

    if !unsafe { sys::init::SDL_Init(sys::init::SDL_INIT_VIDEO) } {
        let error = SdlInstance::error();
        if contexts.is_none() {
            unsafe { sys::init::SDL_Quit() };
        }
        return Err(error);
    }

    let (_, count) = contexts.get_or_insert((current_thread, 0));
    *count += 1;
    Ok(SdlInstance {
        context: Rc::new(SdlContext),
    })
}

impl SdlInstance {
    pub fn poll_event(&mut self) -> Option<WindowEvent> {
        let mut event = MaybeUninit::<sys::events::SDL_Event>::zeroed();
        if unsafe { sys::events::SDL_PollEvent(event.as_mut_ptr()) } {
            Some(unsafe { WindowEvent::from_sdl(event.assume_init_ref()) })
        } else {
            None
        }
    }

    pub fn create_window(
        &self,
        title: &str,
        width: i32,
        height: i32,
        flags: WindowFlags,
    ) -> Result<WindowInstance, WindowError> {
        let title = CString::new(title).map_err(|error| WindowError::Nul(error.to_string()))?;
        let raw =
            unsafe { sys::video::SDL_CreateWindow(title.as_ptr(), width, height, flags.into()) };
        let raw = NonNull::new(raw).ok_or_else(Self::error)?;
        Ok(WindowInstance {
            raw,
            _context: Rc::clone(&self.context),
        })
    }
}

impl SdlInstance {
    fn error() -> WindowError {
        unsafe { copy_string(sys::error::SDL_GetError()) }
            .map(WindowError::Nul)
            .unwrap_or(WindowError::Unknown)
    }
}

impl Drop for WindowInstance {
    fn drop(&mut self) {
        unsafe { sys::video::SDL_DestroyWindow(self.raw.as_ptr()) };
    }
}

impl Drop for SdlContext {
    fn drop(&mut self) {
        let mut contexts = SDL_CONTEXTS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe { sys::init::SDL_QuitSubSystem(sys::init::SDL_INIT_VIDEO) };
        if let Some((_, count)) = contexts.as_mut() {
            *count -= 1;
            if *count == 0 {
                unsafe { sys::init::SDL_Quit() };
                *contexts = None;
            }
        }
    }
}

impl From<sys::video::SDL_WindowID> for WindowId {
    fn from(value: sys::video::SDL_WindowID) -> Self {
        Self(value.0)
    }
}

impl From<WindowId> for sys::video::SDL_WindowID {
    fn from(value: WindowId) -> Self {
        Self(value.0)
    }
}

impl From<sys::video::SDL_DisplayID> for WindowDisplayId {
    fn from(value: sys::video::SDL_DisplayID) -> Self {
        Self(value.0)
    }
}

impl From<WindowDisplayId> for sys::video::SDL_DisplayID {
    fn from(value: WindowDisplayId) -> Self {
        Self(value.0)
    }
}

impl From<sys::properties::SDL_PropertiesID> for WindowPropertiesId {
    fn from(value: sys::properties::SDL_PropertiesID) -> Self {
        Self(value.0)
    }
}

impl From<WindowPropertiesId> for sys::properties::SDL_PropertiesID {
    fn from(value: WindowPropertiesId) -> Self {
        Self(value.0)
    }
}

impl From<sys::pixels::SDL_PixelFormat> for WindowPixelFormat {
    fn from(value: sys::pixels::SDL_PixelFormat) -> Self {
        Self(value.0)
    }
}

impl From<WindowPixelFormat> for sys::pixels::SDL_PixelFormat {
    fn from(value: WindowPixelFormat) -> Self {
        Self(value.0)
    }
}

impl From<u32> for WindowId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<&sys::video::SDL_DisplayMode> for WindowDisplayMode {
    fn from(value: &sys::video::SDL_DisplayMode) -> Self {
        Self {
            display_id: value.displayID.into(),
            format: value.format.into(),
            width: value.w,
            height: value.h,
            pixel_density: value.pixel_density,
            refresh_rate: value.refresh_rate,
            refresh_rate_numerator: value.refresh_rate_numerator,
            refresh_rate_denominator: value.refresh_rate_denominator,
            internal: value.internal.cast(),
        }
    }
}

impl From<sys::video::SDL_DisplayMode> for WindowDisplayMode {
    fn from(value: sys::video::SDL_DisplayMode) -> Self {
        Self::from(&value)
    }
}

impl<'a> From<&'a mut sys::surface::SDL_Surface> for WindowSurfaceRef<'a> {
    fn from(value: &'a mut sys::surface::SDL_Surface) -> Self {
        Self {
            raw: NonNull::from(value).cast(),
            marker: PhantomData,
        }
    }
}

impl From<&WindowSurfaceRef<'_>> for *mut sys::surface::SDL_Surface {
    fn from(value: &WindowSurfaceRef<'_>) -> Self {
        value.raw.cast().as_ptr()
    }
}

impl From<sys::rect::SDL_Point> for WindowPoint {
    fn from(value: sys::rect::SDL_Point) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

impl From<sys::rect::SDL_Rect> for WindowRect {
    fn from(value: sys::rect::SDL_Rect) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.w,
            height: value.h,
        }
    }
}

impl From<WindowPoint> for sys::rect::SDL_Point {
    fn from(value: WindowPoint) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

impl From<WindowRect> for sys::rect::SDL_Rect {
    fn from(value: WindowRect) -> Self {
        Self {
            x: value.x,
            y: value.y,
            w: value.width,
            h: value.height,
        }
    }
}

impl From<sys::video::SDL_WindowFlags> for WindowFlags {
    fn from(value: sys::video::SDL_WindowFlags) -> Self {
        Self::from_bits_retain(value.0)
    }
}

impl From<WindowFlags> for sys::video::SDL_WindowFlags {
    fn from(value: WindowFlags) -> Self {
        Self(value.bits())
    }
}

impl From<sys::video::SDL_FlashOperation> for WindowFlashOperation {
    fn from(value: sys::video::SDL_FlashOperation) -> Self {
        match value {
            sys::video::SDL_FlashOperation::CANCEL => Self::Cancel,
            sys::video::SDL_FlashOperation::BRIEFLY => Self::Briefly,
            sys::video::SDL_FlashOperation::UNTIL_FOCUSED => Self::UntilFocused,
            value => Self::Unknown(value.0),
        }
    }
}

impl From<WindowFlashOperation> for sys::video::SDL_FlashOperation {
    fn from(value: WindowFlashOperation) -> Self {
        match value {
            WindowFlashOperation::Cancel => Self::CANCEL,
            WindowFlashOperation::Briefly => Self::BRIEFLY,
            WindowFlashOperation::UntilFocused => Self::UNTIL_FOCUSED,
            WindowFlashOperation::Unknown(value) => Self(value),
        }
    }
}

impl From<sys::video::SDL_HitTestResult> for WindowHitTestResult {
    fn from(value: sys::video::SDL_HitTestResult) -> Self {
        match value {
            sys::video::SDL_HitTestResult::NORMAL => Self::Normal,
            sys::video::SDL_HitTestResult::DRAGGABLE => Self::Draggable,
            sys::video::SDL_HitTestResult::RESIZE_TOPLEFT => Self::ResizeTopLeft,
            sys::video::SDL_HitTestResult::RESIZE_TOP => Self::ResizeTop,
            sys::video::SDL_HitTestResult::RESIZE_TOPRIGHT => Self::ResizeTopRight,
            sys::video::SDL_HitTestResult::RESIZE_RIGHT => Self::ResizeRight,
            sys::video::SDL_HitTestResult::RESIZE_BOTTOMRIGHT => Self::ResizeBottomRight,
            sys::video::SDL_HitTestResult::RESIZE_BOTTOM => Self::ResizeBottom,
            sys::video::SDL_HitTestResult::RESIZE_BOTTOMLEFT => Self::ResizeBottomLeft,
            sys::video::SDL_HitTestResult::RESIZE_LEFT => Self::ResizeLeft,
            value => Self::Unknown(value.0),
        }
    }
}

impl From<WindowHitTestResult> for sys::video::SDL_HitTestResult {
    fn from(value: WindowHitTestResult) -> Self {
        match value {
            WindowHitTestResult::Normal => Self::NORMAL,
            WindowHitTestResult::Draggable => Self::DRAGGABLE,
            WindowHitTestResult::ResizeTopLeft => Self::RESIZE_TOPLEFT,
            WindowHitTestResult::ResizeTop => Self::RESIZE_TOP,
            WindowHitTestResult::ResizeTopRight => Self::RESIZE_TOPRIGHT,
            WindowHitTestResult::ResizeRight => Self::RESIZE_RIGHT,
            WindowHitTestResult::ResizeBottomRight => Self::RESIZE_BOTTOMRIGHT,
            WindowHitTestResult::ResizeBottom => Self::RESIZE_BOTTOM,
            WindowHitTestResult::ResizeBottomLeft => Self::RESIZE_BOTTOMLEFT,
            WindowHitTestResult::ResizeLeft => Self::RESIZE_LEFT,
            WindowHitTestResult::Unknown(value) => Self(value),
        }
    }
}

impl From<sys::video::SDL_ProgressState> for WindowProgressState {
    fn from(value: sys::video::SDL_ProgressState) -> Self {
        match value {
            sys::video::SDL_ProgressState::INVALID => Self::Invalid,
            sys::video::SDL_ProgressState::NONE => Self::None,
            sys::video::SDL_ProgressState::INDETERMINATE => Self::Indeterminate,
            sys::video::SDL_ProgressState::NORMAL => Self::Normal,
            sys::video::SDL_ProgressState::PAUSED => Self::Paused,
            sys::video::SDL_ProgressState::ERROR => Self::Error,
            value => Self::Unknown(value.0),
        }
    }
}

impl From<WindowProgressState> for sys::video::SDL_ProgressState {
    fn from(value: WindowProgressState) -> Self {
        match value {
            WindowProgressState::Invalid => Self::INVALID,
            WindowProgressState::None => Self::NONE,
            WindowProgressState::Indeterminate => Self::INDETERMINATE,
            WindowProgressState::Normal => Self::NORMAL,
            WindowProgressState::Paused => Self::PAUSED,
            WindowProgressState::Error => Self::ERROR,
            WindowProgressState::Unknown(value) => Self(value),
        }
    }
}

impl From<sys::events::SDL_EventType> for WindowEventCode {
    fn from(value: sys::events::SDL_EventType) -> Self {
        Self(value.0)
    }
}

impl From<sys::keyboard::SDL_KeyboardID> for WindowKeyboardId {
    fn from(value: sys::keyboard::SDL_KeyboardID) -> Self {
        Self(value.0)
    }
}

impl From<sys::mouse::SDL_MouseID> for WindowMouseId {
    fn from(value: sys::mouse::SDL_MouseID) -> Self {
        Self(value.0)
    }
}

impl From<sys::joystick::SDL_JoystickID> for WindowJoystickId {
    fn from(value: sys::joystick::SDL_JoystickID) -> Self {
        Self(value.0)
    }
}

impl From<sys::audio::SDL_AudioDeviceID> for WindowAudioDeviceId {
    fn from(value: sys::audio::SDL_AudioDeviceID) -> Self {
        Self(value.0)
    }
}

impl From<sys::camera::SDL_CameraID> for WindowCameraId {
    fn from(value: sys::camera::SDL_CameraID) -> Self {
        Self(value.0)
    }
}

impl From<sys::sensor::SDL_SensorID> for WindowSensorId {
    fn from(value: sys::sensor::SDL_SensorID) -> Self {
        Self(value.0)
    }
}

impl From<sys::touch::SDL_TouchID> for WindowTouchId {
    fn from(value: sys::touch::SDL_TouchID) -> Self {
        Self(value.0)
    }
}

impl From<sys::touch::SDL_FingerID> for WindowFingerId {
    fn from(value: sys::touch::SDL_FingerID) -> Self {
        Self(value.0)
    }
}

impl From<sys::pen::SDL_PenID> for WindowPenId {
    fn from(value: sys::pen::SDL_PenID) -> Self {
        Self(value.0)
    }
}

impl From<sys::scancode::SDL_Scancode> for WindowScancode {
    fn from(value: sys::scancode::SDL_Scancode) -> Self {
        Self(value.0)
    }
}

impl From<sys::keycode::SDL_Keycode> for WindowKeycode {
    fn from(value: sys::keycode::SDL_Keycode) -> Self {
        Self(value.0)
    }
}

impl From<sys::keycode::SDL_Keymod> for WindowKeymod {
    fn from(value: sys::keycode::SDL_Keymod) -> Self {
        Self(value.0)
    }
}

impl From<sys::mouse::SDL_MouseButtonFlags> for WindowMouseButtonFlags {
    fn from(value: sys::mouse::SDL_MouseButtonFlags) -> Self {
        Self(value.0)
    }
}

impl From<sys::mouse::SDL_MouseWheelDirection> for WindowMouseWheelDirection {
    fn from(value: sys::mouse::SDL_MouseWheelDirection) -> Self {
        Self(value.0)
    }
}

impl From<sys::power::SDL_PowerState> for WindowPowerState {
    fn from(value: sys::power::SDL_PowerState) -> Self {
        Self(value.0)
    }
}

impl From<sys::pen::SDL_PenInputFlags> for WindowPenInputFlags {
    fn from(value: sys::pen::SDL_PenInputFlags) -> Self {
        Self(value.0)
    }
}

impl From<sys::pen::SDL_PenDeviceType> for WindowPenDeviceType {
    fn from(value: sys::pen::SDL_PenDeviceType) -> Self {
        Self(value.0)
    }
}

impl From<sys::pen::SDL_PenAxis> for WindowPenAxis {
    fn from(value: sys::pen::SDL_PenAxis) -> Self {
        Self(value.0)
    }
}

impl From<sys::events::SDL_CommonEvent> for WindowCommonEvent {
    fn from(value: sys::events::SDL_CommonEvent) -> Self {
        Self {
            event_type: value.r#type,
            reserved: value.reserved,
            timestamp: value.timestamp,
        }
    }
}

impl From<sys::events::SDL_DisplayEvent> for WindowDisplayEvent {
    fn from(value: sys::events::SDL_DisplayEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            display_id: value.displayID.into(),
            data1: value.data1,
            data2: value.data2,
        }
    }
}

impl From<sys::events::SDL_KeyboardDeviceEvent> for WindowKeyboardDeviceEvent {
    fn from(value: sys::events::SDL_KeyboardDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
        }
    }
}

impl From<sys::events::SDL_KeyboardEvent> for WindowKeyboardEvent {
    fn from(value: sys::events::SDL_KeyboardEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            scancode: value.scancode.into(),
            key: value.key.into(),
            modifiers: value.r#mod.into(),
            raw: value.raw,
            down: value.down,
            repeat: value.repeat,
        }
    }
}

impl WindowTextEditingEvent {
    /// # Safety
    /// All non-null string pointers and counted arrays must be valid for reading.
    /// Each string must be NUL-terminated and remain valid during this call.
    pub unsafe fn from_sdl(value: &sys::events::SDL_TextEditingEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            text: unsafe { copy_string(value.text) }.unwrap_or_default(),
            start: value.start,
            length: value.length,
        }
    }
}

impl WindowTextEditingCandidatesEvent {
    /// # Safety
    /// All non-null string pointers and counted arrays must be valid for reading.
    /// Each string must be NUL-terminated and remain valid during this call.
    pub unsafe fn from_sdl(value: &sys::events::SDL_TextEditingCandidatesEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            candidates: unsafe { copy_strings(value.candidates, value.num_candidates) },
            num_candidates: value.num_candidates,
            selected_candidate: value.selected_candidate,
            horizontal: value.horizontal,
        }
    }
}

impl WindowTextInputEvent {
    /// # Safety
    /// All non-null string pointers and counted arrays must be valid for reading.
    /// Each string must be NUL-terminated and remain valid during this call.
    pub unsafe fn from_sdl(value: &sys::events::SDL_TextInputEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            text: unsafe { copy_string(value.text) }.unwrap_or_default(),
        }
    }
}

impl From<sys::events::SDL_MouseDeviceEvent> for WindowMouseDeviceEvent {
    fn from(value: sys::events::SDL_MouseDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
        }
    }
}

impl From<sys::events::SDL_MouseMotionEvent> for WindowMouseMotionEvent {
    fn from(value: sys::events::SDL_MouseMotionEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            state: value.state.into(),
            x: value.x,
            y: value.y,
            xrel: value.xrel,
            yrel: value.yrel,
        }
    }
}

impl From<sys::events::SDL_MouseButtonEvent> for WindowMouseButtonEvent {
    fn from(value: sys::events::SDL_MouseButtonEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            button: value.button,
            down: value.down,
            clicks: value.clicks,
            x: value.x,
            y: value.y,
        }
    }
}

impl From<sys::events::SDL_MouseWheelEvent> for WindowMouseWheelEvent {
    fn from(value: sys::events::SDL_MouseWheelEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            x: value.x,
            y: value.y,
            direction: value.direction.into(),
            mouse_x: value.mouse_x,
            mouse_y: value.mouse_y,
            integer_x: value.integer_x,
            integer_y: value.integer_y,
        }
    }
}

impl From<sys::events::SDL_JoyAxisEvent> for WindowJoyAxisEvent {
    fn from(value: sys::events::SDL_JoyAxisEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            axis: value.axis,
            value: value.value,
        }
    }
}

impl From<sys::events::SDL_JoyBallEvent> for WindowJoyBallEvent {
    fn from(value: sys::events::SDL_JoyBallEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            ball: value.ball,
            xrel: value.xrel,
            yrel: value.yrel,
        }
    }
}

impl From<sys::events::SDL_JoyHatEvent> for WindowJoyHatEvent {
    fn from(value: sys::events::SDL_JoyHatEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            hat: value.hat,
            value: value.value,
        }
    }
}

impl From<sys::events::SDL_JoyButtonEvent> for WindowJoyButtonEvent {
    fn from(value: sys::events::SDL_JoyButtonEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            button: value.button,
            down: value.down,
        }
    }
}

impl From<sys::events::SDL_JoyDeviceEvent> for WindowJoyDeviceEvent {
    fn from(value: sys::events::SDL_JoyDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
        }
    }
}

impl From<sys::events::SDL_JoyBatteryEvent> for WindowJoyBatteryEvent {
    fn from(value: sys::events::SDL_JoyBatteryEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            state: value.state.into(),
            percent: value.percent,
        }
    }
}

impl From<sys::events::SDL_GamepadAxisEvent> for WindowGamepadAxisEvent {
    fn from(value: sys::events::SDL_GamepadAxisEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            axis: value.axis,
            value: value.value,
        }
    }
}

impl From<sys::events::SDL_GamepadButtonEvent> for WindowGamepadButtonEvent {
    fn from(value: sys::events::SDL_GamepadButtonEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            button: value.button,
            down: value.down,
        }
    }
}

impl From<sys::events::SDL_GamepadDeviceEvent> for WindowGamepadDeviceEvent {
    fn from(value: sys::events::SDL_GamepadDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
        }
    }
}

impl From<sys::events::SDL_GamepadTouchpadEvent> for WindowGamepadTouchpadEvent {
    fn from(value: sys::events::SDL_GamepadTouchpadEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            touchpad: value.touchpad,
            finger: value.finger,
            x: value.x,
            y: value.y,
            pressure: value.pressure,
        }
    }
}

impl From<sys::events::SDL_GamepadSensorEvent> for WindowGamepadSensorEvent {
    fn from(value: sys::events::SDL_GamepadSensorEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            sensor: value.sensor,
            data: value.data,
            sensor_timestamp: value.sensor_timestamp,
        }
    }
}

impl From<sys::events::SDL_AudioDeviceEvent> for WindowAudioDeviceEvent {
    fn from(value: sys::events::SDL_AudioDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            recording: value.recording,
        }
    }
}

impl From<sys::events::SDL_CameraDeviceEvent> for WindowCameraDeviceEvent {
    fn from(value: sys::events::SDL_CameraDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
        }
    }
}

impl From<sys::events::SDL_RenderEvent> for WindowRenderEvent {
    fn from(value: sys::events::SDL_RenderEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
        }
    }
}

impl From<sys::events::SDL_TouchFingerEvent> for WindowTouchFingerEvent {
    fn from(value: sys::events::SDL_TouchFingerEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            touch_id: value.touchID.into(),
            finger_id: value.fingerID.into(),
            x: value.x,
            y: value.y,
            dx: value.dx,
            dy: value.dy,
            pressure: value.pressure,
            window_id: value.windowID.into(),
        }
    }
}

impl From<sys::events::SDL_PinchFingerEvent> for WindowPinchFingerEvent {
    fn from(value: sys::events::SDL_PinchFingerEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            scale: value.scale,
            window_id: value.windowID.into(),
        }
    }
}

impl From<sys::events::SDL_PenProximityEvent> for WindowPenProximityEvent {
    fn from(value: sys::events::SDL_PenProximityEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            pen_state: value.pen_state.into(),
            device_type: value.device_type.into(),
        }
    }
}

impl From<sys::events::SDL_PenMotionEvent> for WindowPenMotionEvent {
    fn from(value: sys::events::SDL_PenMotionEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            pen_state: value.pen_state.into(),
            x: value.x,
            y: value.y,
            device_type: value.device_type.into(),
        }
    }
}

impl From<sys::events::SDL_PenTouchEvent> for WindowPenTouchEvent {
    fn from(value: sys::events::SDL_PenTouchEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            pen_state: value.pen_state.into(),
            x: value.x,
            y: value.y,
            eraser: value.eraser,
            down: value.down,
            device_type: value.device_type.into(),
        }
    }
}

impl From<sys::events::SDL_PenButtonEvent> for WindowPenButtonEvent {
    fn from(value: sys::events::SDL_PenButtonEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            pen_state: value.pen_state.into(),
            x: value.x,
            y: value.y,
            button: value.button,
            down: value.down,
            device_type: value.device_type.into(),
        }
    }
}

impl From<sys::events::SDL_PenAxisEvent> for WindowPenAxisEvent {
    fn from(value: sys::events::SDL_PenAxisEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            pen_state: value.pen_state.into(),
            x: value.x,
            y: value.y,
            axis: value.axis.into(),
            value: value.value,
            device_type: value.device_type.into(),
        }
    }
}

impl WindowDropEvent {
    /// # Safety
    /// All non-null string pointers and counted arrays must be valid for reading.
    /// Each string must be NUL-terminated and remain valid during this call.
    pub unsafe fn from_sdl(value: &sys::events::SDL_DropEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            x: value.x,
            y: value.y,
            source: unsafe { copy_string(value.source) },
            data: unsafe { copy_string(value.data) },
        }
    }
}

impl WindowClipboardEvent {
    /// # Safety
    /// All non-null string pointers and counted arrays must be valid for reading.
    /// Each string must be NUL-terminated and remain valid during this call.
    pub unsafe fn from_sdl(value: &sys::events::SDL_ClipboardEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            owner: value.owner,
            num_mime_types: value.num_mime_types,
            mime_types: unsafe { copy_strings(value.mime_types, value.num_mime_types) },
        }
    }
}

impl From<sys::events::SDL_SensorEvent> for WindowSensorEvent {
    fn from(value: sys::events::SDL_SensorEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            data: value.data,
            sensor_timestamp: value.sensor_timestamp,
        }
    }
}

impl From<sys::events::SDL_QuitEvent> for WindowQuitEvent {
    fn from(value: sys::events::SDL_QuitEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
        }
    }
}

impl From<sys::events::SDL_UserEvent> for WindowUserEvent {
    fn from(value: sys::events::SDL_UserEvent) -> Self {
        Self {
            event_type: value.r#type,
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            code: value.code,
            data1: value.data1,
            data2: value.data2,
        }
    }
}

impl WindowEvent {
    /// # Safety
    /// The tag must identify an initialized SDL event payload.
    /// Referenced strings and counted arrays must be valid for reading during this call.
    /// User data pointers are copied without being read or taking ownership.
    pub unsafe fn from_sdl(value: &sys::events::SDL_Event) -> Self {
        let event_type = sys::events::SDL_EventType(unsafe { value.r#type });
        match event_type {
            sys::events::SDL_EVENT_QUIT => Self::Quit(unsafe { value.quit }.into()),
            sys::events::SDL_EVENT_TERMINATING => Self::Terminating(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_LOW_MEMORY => Self::LowMemory(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_WILL_ENTER_BACKGROUND => {
                Self::WillEnterBackground(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_DID_ENTER_BACKGROUND => {
                Self::DidEnterBackground(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_WILL_ENTER_FOREGROUND => {
                Self::WillEnterForeground(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_DID_ENTER_FOREGROUND => {
                Self::DidEnterForeground(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_LOCALE_CHANGED => {
                Self::LocaleChanged(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_SYSTEM_THEME_CHANGED => {
                Self::SystemThemeChanged(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_ORIENTATION => {
                Self::DisplayOrientation(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_ADDED => {
                Self::DisplayAdded(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_REMOVED => {
                Self::DisplayRemoved(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_MOVED => {
                Self::DisplayMoved(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_DESKTOP_MODE_CHANGED => {
                Self::DisplayDesktopModeChanged(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_CURRENT_MODE_CHANGED => {
                Self::DisplayCurrentModeChanged(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_CONTENT_SCALE_CHANGED => {
                Self::DisplayContentScaleChanged(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_USABLE_BOUNDS_CHANGED => {
                Self::DisplayUsableBoundsChanged(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_KEY_DOWN => Self::KeyDown(unsafe { value.key }.into()),
            sys::events::SDL_EVENT_KEY_UP => Self::KeyUp(unsafe { value.key }.into()),
            sys::events::SDL_EVENT_TEXT_EDITING => {
                Self::TextEditing(unsafe { WindowTextEditingEvent::from_sdl(&value.edit) })
            }
            sys::events::SDL_EVENT_TEXT_INPUT => {
                Self::TextInput(unsafe { WindowTextInputEvent::from_sdl(&value.text) })
            }
            sys::events::SDL_EVENT_KEYMAP_CHANGED => {
                Self::KeymapChanged(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_KEYBOARD_ADDED => {
                Self::KeyboardAdded(unsafe { value.kdevice }.into())
            }
            sys::events::SDL_EVENT_KEYBOARD_REMOVED => {
                Self::KeyboardRemoved(unsafe { value.kdevice }.into())
            }
            sys::events::SDL_EVENT_TEXT_EDITING_CANDIDATES => Self::TextEditingCandidates(unsafe {
                WindowTextEditingCandidatesEvent::from_sdl(&value.edit_candidates)
            }),
            sys::events::SDL_EVENT_SCREEN_KEYBOARD_SHOWN => {
                Self::ScreenKeyboardShown(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_SCREEN_KEYBOARD_HIDDEN => {
                Self::ScreenKeyboardHidden(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_MOUSE_MOTION => {
                Self::MouseMotion(unsafe { value.motion }.into())
            }
            sys::events::SDL_EVENT_MOUSE_BUTTON_DOWN => {
                Self::MouseButtonDown(unsafe { value.button }.into())
            }
            sys::events::SDL_EVENT_MOUSE_BUTTON_UP => {
                Self::MouseButtonUp(unsafe { value.button }.into())
            }
            sys::events::SDL_EVENT_MOUSE_WHEEL => Self::MouseWheel(unsafe { value.wheel }.into()),
            sys::events::SDL_EVENT_MOUSE_ADDED => Self::MouseAdded(unsafe { value.mdevice }.into()),
            sys::events::SDL_EVENT_MOUSE_REMOVED => {
                Self::MouseRemoved(unsafe { value.mdevice }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_AXIS_MOTION => {
                Self::JoystickAxisMotion(unsafe { value.jaxis }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_BALL_MOTION => {
                Self::JoystickBallMotion(unsafe { value.jball }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_HAT_MOTION => {
                Self::JoystickHatMotion(unsafe { value.jhat }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_BUTTON_DOWN => {
                Self::JoystickButtonDown(unsafe { value.jbutton }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_BUTTON_UP => {
                Self::JoystickButtonUp(unsafe { value.jbutton }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_ADDED => {
                Self::JoystickAdded(unsafe { value.jdevice }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_REMOVED => {
                Self::JoystickRemoved(unsafe { value.jdevice }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_BATTERY_UPDATED => {
                Self::JoystickBatteryUpdated(unsafe { value.jbattery }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_UPDATE_COMPLETE => {
                Self::JoystickUpdateComplete(unsafe { value.jdevice }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_AXIS_MOTION => {
                Self::GamepadAxisMotion(unsafe { value.gaxis }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_BUTTON_DOWN => {
                Self::GamepadButtonDown(unsafe { value.gbutton }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_BUTTON_UP => {
                Self::GamepadButtonUp(unsafe { value.gbutton }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_ADDED => {
                Self::GamepadAdded(unsafe { value.gdevice }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_REMOVED => {
                Self::GamepadRemoved(unsafe { value.gdevice }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_REMAPPED => {
                Self::GamepadRemapped(unsafe { value.gdevice }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_TOUCHPAD_DOWN => {
                Self::GamepadTouchpadDown(unsafe { value.gtouchpad }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_TOUCHPAD_MOTION => {
                Self::GamepadTouchpadMotion(unsafe { value.gtouchpad }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_TOUCHPAD_UP => {
                Self::GamepadTouchpadUp(unsafe { value.gtouchpad }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_SENSOR_UPDATE => {
                Self::GamepadSensorUpdate(unsafe { value.gsensor }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_UPDATE_COMPLETE => {
                Self::GamepadUpdateComplete(unsafe { value.gdevice }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_STEAM_HANDLE_UPDATED => {
                Self::GamepadSteamHandleUpdated(unsafe { value.gdevice }.into())
            }
            sys::events::SDL_EVENT_FINGER_DOWN => Self::FingerDown(unsafe { value.tfinger }.into()),
            sys::events::SDL_EVENT_FINGER_UP => Self::FingerUp(unsafe { value.tfinger }.into()),
            sys::events::SDL_EVENT_FINGER_MOTION => {
                Self::FingerMotion(unsafe { value.tfinger }.into())
            }
            sys::events::SDL_EVENT_FINGER_CANCELED => {
                Self::FingerCanceled(unsafe { value.tfinger }.into())
            }
            sys::events::SDL_EVENT_PINCH_BEGIN => Self::PinchBegin(unsafe { value.pinch }.into()),
            sys::events::SDL_EVENT_PINCH_UPDATE => Self::PinchUpdate(unsafe { value.pinch }.into()),
            sys::events::SDL_EVENT_PINCH_END => Self::PinchEnd(unsafe { value.pinch }.into()),
            sys::events::SDL_EVENT_CLIPBOARD_UPDATE => {
                Self::ClipboardUpdate(unsafe { WindowClipboardEvent::from_sdl(&value.clipboard) })
            }
            sys::events::SDL_EVENT_DROP_FILE => {
                Self::DropFile(unsafe { WindowDropEvent::from_sdl(&value.drop) })
            }
            sys::events::SDL_EVENT_DROP_TEXT => {
                Self::DropText(unsafe { WindowDropEvent::from_sdl(&value.drop) })
            }
            sys::events::SDL_EVENT_DROP_BEGIN => {
                Self::DropBegin(unsafe { WindowDropEvent::from_sdl(&value.drop) })
            }
            sys::events::SDL_EVENT_DROP_COMPLETE => {
                Self::DropComplete(unsafe { WindowDropEvent::from_sdl(&value.drop) })
            }
            sys::events::SDL_EVENT_DROP_POSITION => {
                Self::DropPosition(unsafe { WindowDropEvent::from_sdl(&value.drop) })
            }
            sys::events::SDL_EVENT_AUDIO_DEVICE_ADDED => {
                Self::AudioDeviceAdded(unsafe { value.adevice }.into())
            }
            sys::events::SDL_EVENT_AUDIO_DEVICE_REMOVED => {
                Self::AudioDeviceRemoved(unsafe { value.adevice }.into())
            }
            sys::events::SDL_EVENT_AUDIO_DEVICE_FORMAT_CHANGED => {
                Self::AudioDeviceFormatChanged(unsafe { value.adevice }.into())
            }
            sys::events::SDL_EVENT_SENSOR_UPDATE => {
                Self::SensorUpdate(unsafe { value.sensor }.into())
            }
            sys::events::SDL_EVENT_PEN_PROXIMITY_IN => {
                Self::PenProximityIn(unsafe { value.pproximity }.into())
            }
            sys::events::SDL_EVENT_PEN_PROXIMITY_OUT => {
                Self::PenProximityOut(unsafe { value.pproximity }.into())
            }
            sys::events::SDL_EVENT_PEN_DOWN => Self::PenDown(unsafe { value.ptouch }.into()),
            sys::events::SDL_EVENT_PEN_UP => Self::PenUp(unsafe { value.ptouch }.into()),
            sys::events::SDL_EVENT_PEN_BUTTON_DOWN => {
                Self::PenButtonDown(unsafe { value.pbutton }.into())
            }
            sys::events::SDL_EVENT_PEN_BUTTON_UP => {
                Self::PenButtonUp(unsafe { value.pbutton }.into())
            }
            sys::events::SDL_EVENT_PEN_MOTION => Self::PenMotion(unsafe { value.pmotion }.into()),
            sys::events::SDL_EVENT_PEN_AXIS => Self::PenAxis(unsafe { value.paxis }.into()),
            sys::events::SDL_EVENT_CAMERA_DEVICE_ADDED => {
                Self::CameraDeviceAdded(unsafe { value.cdevice }.into())
            }
            sys::events::SDL_EVENT_CAMERA_DEVICE_REMOVED => {
                Self::CameraDeviceRemoved(unsafe { value.cdevice }.into())
            }
            sys::events::SDL_EVENT_CAMERA_DEVICE_APPROVED => {
                Self::CameraDeviceApproved(unsafe { value.cdevice }.into())
            }
            sys::events::SDL_EVENT_CAMERA_DEVICE_DENIED => {
                Self::CameraDeviceDenied(unsafe { value.cdevice }.into())
            }
            sys::events::SDL_EVENT_RENDER_TARGETS_RESET => {
                Self::RenderTargetsReset(unsafe { value.render }.into())
            }
            sys::events::SDL_EVENT_RENDER_DEVICE_RESET => {
                Self::RenderDeviceReset(unsafe { value.render }.into())
            }
            sys::events::SDL_EVENT_RENDER_DEVICE_LOST => {
                Self::RenderDeviceLost(unsafe { value.render }.into())
            }
            sys::events::SDL_EVENT_PRIVATE0 => Self::Private0(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_PRIVATE1 => Self::Private1(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_PRIVATE2 => Self::Private2(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_PRIVATE3 => Self::Private3(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_POLL_SENTINEL => {
                Self::PollSentinel(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_WINDOW_SHOWN => {
                let window = unsafe { value.window };
                Self::WindowShown {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_HIDDEN => {
                let window = unsafe { value.window };
                Self::WindowHidden {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_EXPOSED => {
                let window = unsafe { value.window };
                Self::WindowExposed {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    live_resize: window.data1 != 0,
                }
            }
            sys::events::SDL_EVENT_WINDOW_MOVED => {
                let window = unsafe { value.window };
                Self::WindowMoved {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    x: window.data1,
                    y: window.data2,
                }
            }
            sys::events::SDL_EVENT_WINDOW_RESIZED => {
                let window = unsafe { value.window };
                Self::WindowResized {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    width: window.data1,
                    height: window.data2,
                }
            }
            sys::events::SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowPixelSizeChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    width: window.data1,
                    height: window.data2,
                }
            }
            sys::events::SDL_EVENT_WINDOW_METAL_VIEW_RESIZED => {
                let window = unsafe { value.window };
                Self::WindowMetalViewResized {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_MINIMIZED => {
                let window = unsafe { value.window };
                Self::WindowMinimized {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_MAXIMIZED => {
                let window = unsafe { value.window };
                Self::WindowMaximized {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_RESTORED => {
                let window = unsafe { value.window };
                Self::WindowRestored {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_MOUSE_ENTER => {
                let window = unsafe { value.window };
                Self::WindowMouseEnter {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_MOUSE_LEAVE => {
                let window = unsafe { value.window };
                Self::WindowMouseLeave {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_FOCUS_GAINED => {
                let window = unsafe { value.window };
                Self::WindowFocusGained {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_FOCUS_LOST => {
                let window = unsafe { value.window };
                Self::WindowFocusLost {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_CLOSE_REQUESTED => {
                let window = unsafe { value.window };
                Self::WindowCloseRequested {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_HIT_TEST => {
                let window = unsafe { value.window };
                Self::WindowHitTest {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    data1: window.data1,
                    data2: window.data2,
                }
            }
            sys::events::SDL_EVENT_WINDOW_ICCPROF_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowICCProfChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_DISPLAY_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowDisplayChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    display_id: WindowDisplayId(window.data1 as u32),
                }
            }
            sys::events::SDL_EVENT_WINDOW_DISPLAY_SCALE_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowDisplayScaleChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_SAFE_AREA_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowSafeAreaChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_OCCLUDED => {
                let window = unsafe { value.window };
                Self::WindowOccluded {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_ENTER_FULLSCREEN => {
                let window = unsafe { value.window };
                Self::WindowEnterFullscreen {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_LEAVE_FULLSCREEN => {
                let window = unsafe { value.window };
                Self::WindowLeaveFullscreen {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_DESTROYED => {
                let window = unsafe { value.window };
                Self::WindowDestroyed {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_HDR_STATE_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowHdrStateChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            event_type
                if (sys::events::SDL_EVENT_USER.0..=sys::events::SDL_EVENT_LAST.0)
                    .contains(&event_type.0) =>
            {
                Self::User(unsafe { value.user }.into())
            }
            _ => Self::Unknown(unsafe { value.common }.into()),
        }
    }
}

unsafe fn copy_string(value: *const c_char) -> Option<String> {
    if value.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(value) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

unsafe fn copy_strings(values: *const *const c_char, count: i32) -> Vec<String> {
    if count <= 0 {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(values, count as usize) }
        .iter()
        .map(|value| unsafe { copy_string(*value) }.unwrap_or_default())
        .collect()
}
