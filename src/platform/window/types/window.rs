//! Window identity and borrowed native handles.

pub type WindowHandle<'a> = raw_window_handle::WindowHandle<'a>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowId(pub u32);
