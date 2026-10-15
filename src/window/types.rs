use std::{ffi::c_void, marker::PhantomData, ptr::NonNull};

pub type WindowDisplayHandle<'a> = raw_window_handle::DisplayHandle<'a>;
pub type WindowHandle<'a> = raw_window_handle::WindowHandle<'a>;
pub type WindowHandleError = raw_window_handle::HandleError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowDisplayId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowPropertiesId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowPixelFormat(pub i32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowDisplayMode {
    pub display_id: WindowDisplayId,
    pub format: WindowPixelFormat,
    pub width: i32,
    pub height: i32,
    pub pixel_density: f32,
    pub refresh_rate: f32,
    pub refresh_rate_numerator: i32,
    pub refresh_rate_denominator: i32,
    pub internal: *mut c_void,
}

pub struct WindowSurfaceRef<'a> {
    pub(super) raw: NonNull<c_void>,
    pub(super) marker: PhantomData<&'a mut ()>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowFlashOperation {
    Cancel,
    Briefly,
    UntilFocused,
    Unknown(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowHitTestResult {
    Normal,
    Draggable,
    ResizeTopLeft,
    ResizeTop,
    ResizeTopRight,
    ResizeRight,
    ResizeBottomRight,
    ResizeBottom,
    ResizeBottomLeft,
    ResizeLeft,
    Unknown(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowProgressState {
    Invalid,
    None,
    Indeterminate,
    Normal,
    Paused,
    Error,
    Unknown(i32),
}
