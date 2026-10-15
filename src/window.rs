use std::{any::Any, fmt};

pub type WindowMonitorHandle = winit::monitor::MonitorHandle;
pub type WindowCursor = winit::window::Cursor;
pub type WindowCursorGrabMode = winit::window::CursorGrabMode;
pub type WindowCursorIcon = winit::window::CursorIcon;
pub type WindowFullscreen = winit::window::Fullscreen;
pub type WindowIcon = winit::window::Icon;
pub type WindowImePurpose = winit::window::ImePurpose;
pub type WindowTheme = winit::window::Theme;
pub type WindowUserAttentionType = winit::window::UserAttentionType;
pub type WindowButtons = winit::window::WindowButtons;
pub type WindowLevel = winit::window::WindowLevel;
pub type WindowId = winit::window::WindowId;
pub type WindowEvent = winit::event::WindowEvent;
pub type WindowResizeDirection = winit::window::ResizeDirection;
pub type WindowDisplayHandle<'a> = raw_window_handle::DisplayHandle<'a>;
pub type WindowHandle<'a> = raw_window_handle::WindowHandle<'a>;
pub type WindowHandleError = raw_window_handle::HandleError;

#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error(transparent)]
    Request(#[from] winit::error::ExternalError),
    #[error(transparent)]
    Handle(#[from] WindowHandleError),
}

impl From<winit::error::NotSupportedError> for WindowError {
    fn from(error: winit::error::NotSupportedError) -> Self {
        Self::Request(winit::error::ExternalError::NotSupported(error))
    }
}

pub trait Window: Any + Send + Sync + fmt::Debug {
    fn id(&self) -> WindowId;

    fn scale_factor(&self) -> f64;

    fn request_redraw(&self);

    fn pre_present_notify(&self);

    fn reset_dead_keys(&self);

    fn surface_position(&self) -> (i32, i32);

    fn outer_position(&self) -> Result<(i32, i32), WindowError>;

    fn set_outer_position(&self, x: i32, y: i32);

    fn surface_size(&self) -> (u32, u32);

    #[must_use]
    fn request_surface_size(&self, width: u32, height: u32) -> Option<(u32, u32)>;

    fn outer_size(&self) -> (u32, u32);

    fn safe_area(&self) -> (u32, u32, u32, u32);

    fn set_min_surface_size(&self, min_size: Option<(u32, u32)>);

    fn set_max_surface_size(&self, max_size: Option<(u32, u32)>);

    fn surface_resize_increments(&self) -> Option<(u32, u32)>;

    fn set_surface_resize_increments(&self, increments: Option<(u32, u32)>);

    fn set_title(&self, title: &str);

    fn set_transparent(&self, transparent: bool);

    fn set_blur(&self, blur: bool);

    fn set_visible(&self, visible: bool);

    fn is_visible(&self) -> Option<bool>;

    fn set_resizable(&self, resizable: bool);

    fn is_resizable(&self) -> bool;

    fn set_enabled_buttons(&self, buttons: WindowButtons);

    fn enabled_buttons(&self) -> WindowButtons;

    fn set_minimized(&self, minimized: bool);

    fn is_minimized(&self) -> Option<bool>;

    fn set_maximized(&self, maximized: bool);

    fn is_maximized(&self) -> bool;

    fn set_fullscreen(&self, fullscreen: Option<WindowFullscreen>);

    fn fullscreen(&self) -> Option<WindowFullscreen>;

    fn set_decorations(&self, decorations: bool);

    fn is_decorated(&self) -> bool;

    fn set_window_level(&self, level: WindowLevel);

    fn set_window_icon(&self, window_icon: Option<WindowIcon>);

    fn set_ime_cursor_area(&self, x: i32, y: i32, width: u32, height: u32);

    fn set_ime_allowed(&self, allowed: bool);

    fn set_ime_purpose(&self, purpose: WindowImePurpose);

    fn focus_window(&self);

    fn has_focus(&self) -> bool;

    fn request_user_attention(&self, request_type: Option<WindowUserAttentionType>);

    fn set_theme(&self, theme: Option<WindowTheme>);

    fn theme(&self) -> Option<WindowTheme>;

    fn set_content_protected(&self, protected: bool);

    fn title(&self) -> String;

    fn set_cursor(&self, cursor: WindowCursor);

    fn set_cursor_position(&self, x: f64, y: f64) -> Result<(), WindowError>;

    fn set_cursor_grab(&self, mode: WindowCursorGrabMode) -> Result<(), WindowError>;

    fn set_cursor_visible(&self, visible: bool);

    fn drag_window(&self) -> Result<(), WindowError>;

    fn drag_resize_window(&self, direction: WindowResizeDirection) -> Result<(), WindowError>;

    fn show_window_menu(&self, x: i32, y: i32);

    fn set_cursor_hittest(&self, hittest: bool) -> Result<(), WindowError>;

    fn current_monitor(&self) -> Option<WindowMonitorHandle>;

    fn available_monitors(&self) -> Box<dyn Iterator<Item = WindowMonitorHandle>>;

    fn primary_monitor(&self) -> Option<WindowMonitorHandle>;

    fn display_handle(&self) -> Result<WindowDisplayHandle<'_>, WindowError>;

    fn window_handle(&self) -> Result<WindowHandle<'_>, WindowError>;
}
