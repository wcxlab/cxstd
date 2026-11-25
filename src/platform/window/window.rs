//! Window construction, handler lifecycle, and borrowed presentation frames.

use super::types::*;
use super::{WindowError, WindowHandlerTrait, WindowTrait};
use crate::platform::backend::sdl::{
    self, WindowDisplayMode, WindowFlashOperation, WindowPixelFormat, WindowProgressState,
    WindowPropertiesId, WindowRect, WindowSurfaceRef,
};
use crate::platform::{PlatformEvent, PlatformEventMessage};
use crate::platform::platform::{register_window, unregister_window, window_context};
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::{Rc, Weak};

/// A cloneable reference to a platform-owned window on the current thread.
/// Dropping references leaves the window open until it is closed or the platform thread exits.
/// After closing, event handling, updates, and rendering are skipped. The ID
/// and `is_closed()` remain available. Other infallible methods require a live window.
#[derive(Clone)]
pub struct WindowRef {
    state: Weak<WindowState>,
    id: WindowId,
    frame: Rc<sdl::FrameSlot>,
}

pub type Window = WindowRef;

pub(in crate::platform) struct WindowState {
    // Release handler captures before releasing GPU and native resources.
    handler: RefCell<Option<Box<dyn WindowHandlerTrait<WindowRef>>>>,
    native: RefCell<sdl::SdlWindow<true>>,
    close_requested: Cell<bool>,
    closed: Cell<bool>,
}

impl WindowRef {
    pub fn builder() -> WindowBuilder {
        WindowBuilder::new()
    }
    pub fn surface_config(&self) -> wgpu::SurfaceConfiguration {
        self.read_native(|native| native.surface_config().clone())
    }
    pub fn clear_color(&self) -> wgpu::Color {
        self.read_native(sdl::SdlWindow::clear_color)
    }
    pub fn set_clear_color(&mut self, color: wgpu::Color) {
        self.with_native_mut(|native| {
            native.set_clear_color(color);
            Ok(())
        })
        .expect("a live window is required");
    }
    pub fn resize_surface(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.resize_surface(width, height))
    }
    pub fn next_frame(&self) -> anyhow::Result<WindowFrame<'_>> {
        Ok(WindowFrame {
            _state: self.state()?,
            inner: self.frame.borrow()?,
        })
    }

    pub fn is_closed(&self) -> bool {
        self.state().is_err()
    }

    pub fn handle_event(&mut self, event: &PlatformEventMessage) -> anyhow::Result<()> {
        if self.is_closed() {
            return Ok(());
        }
        self.with_handler(|handler, window| {
            let result = (|| {
                if event.window_id != window.id() {
                    return Ok(());
                }
                if matches!(
                    event.event,
                    PlatformEvent::Resize(_)
                        | PlatformEvent::DevicePixelRatioChange(_)
                        | PlatformEvent::Move(_)
                        | PlatformEvent::Focus
                        | PlatformEvent::Blur
                        | PlatformEvent::Redraw
                        | PlatformEvent::Minimize
                        | PlatformEvent::Maximize
                        | PlatformEvent::Restore
                        | PlatformEvent::FullscreenChange(_)
                ) {
                    window.with_native_mut(sdl::SdlWindow::refresh_metrics)?;
                }
                let pixel_size = match &event.event {
                    PlatformEvent::Resize(event) => {
                        Some((event.inner_physical_width, event.inner_physical_height))
                    }
                    PlatformEvent::DevicePixelRatioChange(event) => {
                        Some((event.inner_physical_width, event.inner_physical_height))
                    }
                    _ => None,
                };
                if let Some((width, height)) = pixel_size {
                    window.resize_surface(width, height)?;
                }
                handler.event(window, event)
            })();
            result.or_else(|error| handler.error(error))
        })
    }

    pub fn render(&mut self) -> anyhow::Result<bool> {
        if self.is_closed() {
            return Ok(false);
        }
        self.with_handler(|handler, window| {
            match window.with_native(sdl::SdlWindow::begin_frame) {
                Ok(true) => {}
                Ok(false) => return Ok(false),
                Err(error) => {
                    handler.error(error)?;
                    return Ok(false);
                }
            }
            let frame_window = window.clone();
            let result = catch_unwind(AssertUnwindSafe(|| handler.render(window)));
            let result = match result {
                Ok(result) => result,
                Err(payload) => {
                    let _ = frame_window.with_native(|native| native.finish_frame(false));
                    resume_unwind(payload);
                }
            };
            frame_window.with_native(|native| native.finish_frame(result.is_ok()))?;
            if let Err(error) = result {
                handler.error(error)?;
                return Ok(false);
            }
            handler
                .frame_ready(window)
                .or_else(|error| handler.error(error))?;
            Ok(true)
        })
    }
}

/// A borrowed GPU frame available only inside the handler's render callback.
pub struct WindowFrame<'a> {
    inner: sdl::SdlWindowFrame<'a>,
    _state: Rc<WindowState>,
}

impl WindowFrame<'_> {
    pub fn texture_view(&self) -> &wgpu::TextureView {
        self.inner.texture_view()
    }
    pub fn command_encoder(&mut self) -> &mut wgpu::CommandEncoder {
        self.inner.command_encoder()
    }
}

impl WindowTrait for WindowRef {
    fn id(&self) -> WindowId {
        self.id
    }
    fn inner_width(&self) -> f32 {
        self.read_native(sdl::SdlWindow::inner_width)
    }
    fn inner_height(&self) -> f32 {
        self.read_native(sdl::SdlWindow::inner_height)
    }
    fn inner_physical_width(&self) -> u32 {
        self.read_native(sdl::SdlWindow::inner_physical_width)
    }
    fn inner_physical_height(&self) -> u32 {
        self.read_native(sdl::SdlWindow::inner_physical_height)
    }
    fn screen_x(&self) -> f32 {
        self.read_native(sdl::SdlWindow::screen_x)
    }
    fn screen_y(&self) -> f32 {
        self.read_native(sdl::SdlWindow::screen_y)
    }
    fn screen_physical_x(&self) -> i32 {
        self.read_native(sdl::SdlWindow::screen_physical_x)
    }
    fn screen_physical_y(&self) -> i32 {
        self.read_native(sdl::SdlWindow::screen_physical_y)
    }
    fn device_pixel_ratio(&self) -> f32 {
        self.read_native(sdl::SdlWindow::device_pixel_ratio)
    }

    fn resize_to(&mut self, width: i32, height: i32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.resize_to(width, height))
    }

    fn resize_by(&mut self, width: i32, height: i32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.resize_by(width, height))
    }

    fn move_to(&mut self, x: i32, y: i32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.move_to(x, y))
    }

    fn move_by(&mut self, x: i32, y: i32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.move_by(x, y))
    }

    fn maximize(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::maximize)
    }

    fn is_maximized(&self) -> bool {
        self.read_native(sdl::SdlWindow::is_maximized)
    }

    fn minimize(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::minimize)
    }

    fn is_minimized(&self) -> bool {
        self.read_native(sdl::SdlWindow::is_minimized)
    }

    fn restore(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::restore)
    }

    fn focus(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::focus)
    }

    fn is_focused(&self) -> bool {
        self.read_native(sdl::SdlWindow::is_focused)
    }

    fn redraw(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::redraw)
    }

    fn is_closed(&self) -> bool {
        WindowRef::is_closed(self)
    }

    fn close(&mut self) -> anyhow::Result<()> {
        if let Ok(state) = self.state() {
            let handler = state.handler.borrow_mut().take();
            if let Some(handler) = handler {
                state.closed.set(true);
                unregister_window(self.id);
                drop(handler);
            } else {
                state.close_requested.set(true);
            }
        }
        Ok(())
    }

    fn set_cursor(&mut self, cursor: WindowCursor) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_cursor(cursor))
    }
    fn start_text_input(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::start_text_input)
    }
    fn stop_text_input(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::stop_text_input)
    }

    fn fullscreen(&mut self, fullscreen: bool) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.fullscreen(fullscreen))
    }

    fn is_fullscreen(&self) -> bool {
        self.read_native(sdl::SdlWindow::is_fullscreen)
    }

    fn borderless(&mut self, borderless: bool) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.borderless(borderless))
    }

    fn is_borderless(&self) -> bool {
        self.read_native(sdl::SdlWindow::is_borderless)
    }

    fn raw_window_handle(&self) -> anyhow::Result<WindowHandle> {
        self.with_native(sdl::SdlWindow::raw_window_handle)
    }
}

impl WindowRef {
    pub fn update(&mut self) -> anyhow::Result<()> {
        if self.is_closed() {
            return Ok(());
        }
        self.with_handler(|handler, window| {
            let result = window
                .with_native_mut(sdl::SdlWindow::refresh_metrics)
                .and_then(|()| handler.update(window));
            result.or_else(|error| handler.error(error))
        })
    }

    pub fn window_handle_provider(&self) -> Rc<dyn raw_window_handle::HasWindowHandle> {
        self.read_native(sdl::SdlWindow::window_handle_provider)
    }
}

// Retain native operations privately for future host needs.
#[allow(dead_code)]
impl WindowRef {
    fn show_system_menu(&mut self, x: i32, y: i32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.show_system_menu(x, y))
    }

    fn get_pixel_density(&self) -> anyhow::Result<f32> {
        self.with_native(sdl::SdlWindow::get_pixel_density)
    }

    fn get_display_scale(&self) -> anyhow::Result<f32> {
        self.with_native(sdl::SdlWindow::get_display_scale)
    }

    fn get_display(&self) -> anyhow::Result<u32> {
        self.with_native(sdl::SdlWindow::get_display)
    }
    fn set_fullscreen_mode(&mut self, mode: Option<&WindowDisplayMode>) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_fullscreen_mode(mode))
    }
    fn get_fullscreen_mode(&self) -> Option<WindowDisplayMode> {
        self.read_native(sdl::SdlWindow::get_fullscreen_mode)
    }
    fn get_icc_profile(&self) -> anyhow::Result<Vec<u8>> {
        self.with_native(sdl::SdlWindow::get_icc_profile)
    }
    fn get_pixel_format(&self) -> anyhow::Result<WindowPixelFormat> {
        self.with_native(sdl::SdlWindow::get_pixel_format)
    }
    fn get_properties(&self) -> anyhow::Result<WindowPropertiesId> {
        self.with_native(sdl::SdlWindow::get_properties)
    }
    fn is_hidden(&self) -> bool {
        self.read_native(sdl::SdlWindow::is_hidden)
    }
    fn is_resizable(&self) -> bool {
        self.read_native(sdl::SdlWindow::is_resizable)
    }
    fn set_title(&mut self, title: &str) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_title(title))
    }
    fn get_title(&self) -> String {
        self.read_native(sdl::SdlWindow::get_title)
    }
    fn set_icon(&mut self, icon: &WindowSurfaceRef<'_>) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_icon(icon))
    }
    fn get_position(&self) -> anyhow::Result<(i32, i32)> {
        self.with_native(sdl::SdlWindow::get_position)
    }
    fn get_size(&self) -> anyhow::Result<(i32, i32)> {
        self.with_native(sdl::SdlWindow::get_size)
    }
    fn get_safe_area(&self) -> anyhow::Result<WindowRect> {
        self.with_native(sdl::SdlWindow::get_safe_area)
    }
    fn set_aspect_ratio(&mut self, min_aspect: f32, max_aspect: f32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_aspect_ratio(min_aspect, max_aspect))
    }
    fn get_aspect_ratio(&self) -> anyhow::Result<(f32, f32)> {
        self.with_native(sdl::SdlWindow::get_aspect_ratio)
    }
    fn get_borders_size(&self) -> anyhow::Result<(i32, i32, i32, i32)> {
        self.with_native(sdl::SdlWindow::get_borders_size)
    }
    fn get_size_in_pixels(&self) -> anyhow::Result<(i32, i32)> {
        self.with_native(sdl::SdlWindow::get_size_in_pixels)
    }
    fn set_minimum_size(&mut self, width: i32, height: i32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_minimum_size(width, height))
    }
    fn get_minimum_size(&self) -> anyhow::Result<(i32, i32)> {
        self.with_native(sdl::SdlWindow::get_minimum_size)
    }
    fn set_maximum_size(&mut self, width: i32, height: i32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_maximum_size(width, height))
    }
    fn get_maximum_size(&self) -> anyhow::Result<(i32, i32)> {
        self.with_native(sdl::SdlWindow::get_maximum_size)
    }
    fn set_resizable(&mut self, resizable: bool) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_resizable(resizable))
    }
    fn set_always_on_top(&mut self, on_top: bool) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_always_on_top(on_top))
    }
    fn set_fill_document(&mut self, fill: bool) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_fill_document(fill))
    }
    fn show(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::show)
    }
    fn hide(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::hide)
    }
    fn sync(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::sync)
    }
    fn has_surface(&self) -> bool {
        self.read_native(sdl::SdlWindow::has_surface)
    }
    fn with_surface<R>(&mut self, read: impl FnOnce(WindowSurfaceRef<'_>) -> R) -> anyhow::Result<R> {
        self.with_native_mut(|native| Ok(read(native.get_surface()?)))
    }
    fn set_surface_vsync(&mut self, vsync: i32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_surface_vsync(vsync))
    }
    fn get_surface_vsync(&self) -> anyhow::Result<i32> {
        self.with_native(sdl::SdlWindow::get_surface_vsync)
    }
    fn update_surface(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::update_surface)
    }
    fn update_surface_rects(&mut self, rects: &[WindowRect]) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.update_surface_rects(rects))
    }
    fn destroy_surface(&mut self) -> anyhow::Result<()> {
        self.with_native_mut(sdl::SdlWindow::destroy_surface)
    }
    fn set_keyboard_grab(&mut self, grabbed: bool) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_keyboard_grab(grabbed))
    }
    fn set_mouse_grab(&mut self, grabbed: bool) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_mouse_grab(grabbed))
    }
    fn get_keyboard_grab(&self) -> bool {
        self.read_native(sdl::SdlWindow::get_keyboard_grab)
    }
    fn get_mouse_grab(&self) -> bool {
        self.read_native(sdl::SdlWindow::get_mouse_grab)
    }
    fn set_mouse_rect(&mut self, rect: Option<&WindowRect>) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_mouse_rect(rect))
    }
    fn get_mouse_rect(&self) -> Option<WindowRect> {
        self.read_native(sdl::SdlWindow::get_mouse_rect)
    }
    fn set_opacity(&mut self, opacity: f32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_opacity(opacity))
    }
    fn get_opacity(&self) -> anyhow::Result<f32> {
        self.with_native(sdl::SdlWindow::get_opacity)
    }
    fn set_focusable(&mut self, focusable: bool) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_focusable(focusable))
    }
    fn set_shape(&mut self, shape: Option<&WindowSurfaceRef<'_>>) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_shape(shape))
    }
    fn flash(&mut self, operation: WindowFlashOperation) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.flash(operation))
    }
    fn set_progress_state(&mut self, state: WindowProgressState) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_progress_state(state))
    }
    fn get_progress_state(&self) -> anyhow::Result<WindowProgressState> {
        self.with_native(sdl::SdlWindow::get_progress_state)
    }
    fn set_progress_value(&mut self, value: f32) -> anyhow::Result<()> {
        self.with_native_mut(|native| native.set_progress_value(value))
    }
    fn get_progress_value(&self) -> anyhow::Result<f32> {
        self.with_native(sdl::SdlWindow::get_progress_value)
    }
    fn with_display_handle<R>(
        &self,
        read: impl FnOnce(raw_window_handle::DisplayHandle<'_>) -> R,
    ) -> anyhow::Result<R> {
        self.with_native(|native| Ok(read(native.display_handle()?)))
    }
}

impl WindowRef {
    fn state(&self) -> anyhow::Result<Rc<WindowState>> {
        self.state
            .upgrade()
            .filter(|state| !state.closed.get())
            .ok_or_else(|| WindowError::Closed.into())
    }

    fn with_native<R>(
        &self,
        access: impl FnOnce(&sdl::SdlWindow<true>) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        let state = self.state()?;
        let native = state
            .native
            .try_borrow()
            .map_err(|error| WindowError::Nul(error.to_string()))?;
        access(&native)
    }

    fn with_native_mut<R>(
        &self,
        access: impl FnOnce(&mut sdl::SdlWindow<true>) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        let state = self.state()?;
        let mut native = state
            .native
            .try_borrow_mut()
            .map_err(|error| WindowError::Nul(error.to_string()))?;
        access(&mut native)
    }

    fn read_native<R>(&self, access: impl FnOnce(&sdl::SdlWindow<true>) -> R) -> R {
        self.with_native(|native| Ok(access(native)))
            .expect("a live window is required")
    }

    fn initialize<H: WindowHandlerTrait<Self> + 'static>(
        native: sdl::SdlWindow<true>,
        handler: H,
    ) -> anyhow::Result<Self> {
        let id = native.id();
        let frame = native.frame_slot();
        let state = Rc::new(WindowState {
            handler: RefCell::new(Some(Box::new(handler))),
            native: RefCell::new(native),
            close_requested: Cell::new(false),
            closed: Cell::new(false),
        });
        let mut window = Self {
            state: Rc::downgrade(&state),
            id,
            frame,
        };
        register_window(id, state);
        let initialized = catch_unwind(AssertUnwindSafe(|| {
            window.with_handler(|handler, window| {
                handler.init(window).or_else(|error| handler.error(error))
            })?;
            if !window.is_closed() {
                window.redraw()?;
            }
            Ok(())
        }));
        match initialized {
            Ok(Ok(())) => Ok(window),
            Ok(Err(error)) => {
                window.close()?;
                Err(error)
            }
            Err(payload) => {
                let _ = window.close();
                resume_unwind(payload)
            }
        }
    }

    fn with_handler<T>(
        &mut self,
        callback: impl FnOnce(&mut dyn WindowHandlerTrait<Self>, &mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let state = self.state()?;
        let mut handler = state
            .handler
            .borrow_mut()
            .take()
            .ok_or_else(|| WindowError::Nul("a window callback is already active".into()))?;
        let mut window = self.clone();
        let result = catch_unwind(AssertUnwindSafe(|| callback(handler.as_mut(), &mut window)));
        *state.handler.borrow_mut() = Some(handler);
        if state.close_requested.get() {
            self.close()?;
        }
        if !Weak::ptr_eq(&self.state, &window.state) && result.is_ok() {
            return Err(WindowError::Nul("the callback replaced the active window".into()).into());
        }
        match result {
            Ok(result) => result.map_err(|error| WindowError::Callback(error).into()),
            Err(payload) => resume_unwind(payload),
        }
    }
}

/// Owns window configuration and a handler until build initializes the window.
/// Settings can be applied before or after handler(); no handler uses empty callbacks.
pub struct WindowBuilder<H = ()> {
    title: String,
    position: (i32, i32),
    size: (i32, i32),
    resizable: bool,
    hidden: bool,
    handler: H,
}

impl<H> WindowBuilder<H> {
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
    /// Set the position in the platform's desktop coordinates.
    pub fn position(mut self, x: i32, y: i32) -> Self {
        self.position = (x, y);
        self
    }
    /// Set the client size in logical pixels.
    pub fn size(mut self, width: i32, height: i32) -> Self {
        self.size = (width, height);
        self
    }
    pub fn resizable(mut self, enabled: bool) -> Self {
        self.resizable = enabled;
        self
    }
    pub fn hidden(mut self, enabled: bool) -> Self {
        self.hidden = enabled;
        self
    }
    pub fn handler<T>(self, handler: T) -> WindowBuilder<T> {
        WindowBuilder {
            title: self.title,
            position: self.position,
            size: self.size,
            resizable: self.resizable,
            hidden: self.hidden,
            handler,
        }
    }
}

impl<H: WindowHandlerTrait<WindowRef> + 'static> WindowBuilder<H> {
    pub fn build(self) -> anyhow::Result<WindowRef> {
        let native = sdl::SdlInstance {
            context: window_context()?,
        };
        let inner = native.create_window(
            &self.title,
            self.position,
            self.size,
            self.resizable,
            self.hidden,
        )?;
        WindowRef::initialize(inner, self.handler)
    }
}

impl WindowBuilder {
    pub(in crate::platform) fn new() -> Self {
        Self {
            title: String::from("cxstd"),
            position: (100, 100),
            size: (800, 600),
            resizable: false,
            hidden: false,
            handler: (),
        }
    }
}
