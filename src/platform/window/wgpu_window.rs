//! WGPU window construction, handler lifecycle, and borrowed presentation frames.

use super::types::*;
use super::{Window, WindowError, WindowHandler};
use crate::platform::backend::sdl::{
    self, WindowDisplayMode, WindowFlashOperation, WindowPixelFormat, WindowProgressState,
    WindowPropertiesId, WindowRect, WindowSurfaceRef,
};
use crate::platform::{PlatformEvent, PlatformEventMessage};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;

/// A GPU window owning its handler, device, frame state, and native window.
pub struct WgpuWindow {
    // Release handler captures before releasing GPU and native resources.
    pub(super) handler: Option<Box<dyn WindowHandler<WgpuWindow>>>,
    pub(super) inner: sdl::SdlWindow<true>,
}

impl WgpuWindow {
    pub fn builder() -> WgpuWindowBuilder {
        WgpuWindowBuilder::new()
    }
    pub fn instance(&self) -> &wgpu::Instance {
        self.inner.instance()
    }
    pub fn adapter(&self) -> &wgpu::Adapter {
        self.inner.adapter()
    }
    pub fn device(&self) -> &wgpu::Device {
        self.inner.device()
    }
    pub fn queue(&self) -> &wgpu::Queue {
        self.inner.queue()
    }
    pub fn surface_config(&self) -> &wgpu::SurfaceConfiguration {
        self.inner.surface_config()
    }
    pub fn clear_color(&self) -> wgpu::Color {
        self.inner.clear_color()
    }
    pub fn set_clear_color(&mut self, color: wgpu::Color) {
        self.inner.set_clear_color(color);
    }
    pub fn resize_surface(&mut self, width: u32, height: u32) -> Result<(), WindowError> {
        self.inner.resize_surface(width, height)
    }
    pub fn next_frame(&self) -> Result<WgpuWindowFrame<'_>, WindowError> {
        Ok(WgpuWindowFrame {
            inner: self.inner.next_frame()?,
        })
    }

    pub fn window_event(&mut self, event: &PlatformEventMessage) -> Result<(), WindowError> {
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
                    window.inner.refresh_metrics()?;
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
                handler.window_event(window, event)
            })();
            result.or_else(|error| handler.error(error))
        })
    }

    pub fn render(&mut self) -> Result<bool, WindowError> {
        self.with_handler(|handler, window| {
            match window.inner.begin_frame() {
                Ok(true) => {}
                Ok(false) => return Ok(false),
                Err(error) => {
                    handler.error(error.into())?;
                    return Ok(false);
                }
            }
            let result = catch_unwind(AssertUnwindSafe(|| handler.render(window)));
            let result = match result {
                Ok(result) => result,
                Err(payload) => {
                    let _ = window.inner.finish_frame(false);
                    resume_unwind(payload);
                }
            };
            window.inner.finish_frame(result.is_ok())?;
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
pub struct WgpuWindowFrame<'a> {
    inner: sdl::SdlWgpuFrame<'a>,
}

impl WgpuWindowFrame<'_> {
    pub fn texture_view(&self) -> &wgpu::TextureView {
        self.inner.texture_view()
    }
    pub fn command_encoder(&mut self) -> &mut wgpu::CommandEncoder {
        self.inner.command_encoder()
    }
}

impl Window for WgpuWindow {
    fn id(&self) -> WindowId {
        self.inner.id()
    }
    fn inner_width(&self) -> f32 {
        self.inner.inner_width()
    }
    fn inner_height(&self) -> f32 {
        self.inner.inner_height()
    }
    fn inner_physical_width(&self) -> u32 {
        self.inner.inner_physical_width()
    }
    fn inner_physical_height(&self) -> u32 {
        self.inner.inner_physical_height()
    }
    fn screen_x(&self) -> f32 {
        self.inner.screen_x()
    }
    fn screen_y(&self) -> f32 {
        self.inner.screen_y()
    }
    fn screen_physical_x(&self) -> i32 {
        self.inner.screen_physical_x()
    }
    fn screen_physical_y(&self) -> i32 {
        self.inner.screen_physical_y()
    }
    fn device_pixel_ratio(&self) -> f32 {
        self.inner.device_pixel_ratio()
    }

    fn resize_to(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        self.inner.resize_to(width, height)
    }

    fn resize_by(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        self.inner.resize_by(width, height)
    }

    fn move_to(&mut self, x: i32, y: i32) -> Result<(), WindowError> {
        self.inner.move_to(x, y)
    }

    fn move_by(&mut self, x: i32, y: i32) -> Result<(), WindowError> {
        self.inner.move_by(x, y)
    }

    fn maximize(&mut self) -> Result<(), WindowError> {
        self.inner.maximize()
    }

    fn is_maximized(&self) -> bool {
        self.inner.is_maximized()
    }

    fn minimize(&mut self) -> Result<(), WindowError> {
        self.inner.minimize()
    }

    fn is_minimized(&self) -> bool {
        self.inner.is_minimized()
    }

    fn restore(&mut self) -> Result<(), WindowError> {
        self.inner.restore()
    }

    fn focus(&mut self) -> Result<(), WindowError> {
        self.inner.focus()
    }

    fn is_focused(&self) -> bool {
        self.inner.is_focused()
    }

    fn request_redraw(&mut self) -> Result<(), WindowError> {
        self.inner.request_redraw()
    }
    fn request_close(&mut self) -> Result<(), WindowError> {
        self.inner.request_close()
    }

    fn set_cursor(&mut self, cursor: WindowCursor) -> Result<(), WindowError> {
        self.inner.set_cursor(cursor)
    }
    fn start_text_input(&mut self) -> Result<(), WindowError> {
        self.inner.start_text_input()
    }
    fn stop_text_input(&mut self) -> Result<(), WindowError> {
        self.inner.stop_text_input()
    }

    fn fullscreen(&mut self, fullscreen: bool) -> Result<(), WindowError> {
        self.inner.fullscreen(fullscreen)
    }

    fn is_fullscreen(&self) -> bool {
        self.inner.is_fullscreen()
    }

    fn borderless(&mut self, borderless: bool) -> Result<(), WindowError> {
        self.inner.borderless(borderless)
    }

    fn is_borderless(&self) -> bool {
        self.inner.is_borderless()
    }

    fn raw_window_handle(&self) -> Result<WindowHandle<'_>, WindowError> {
        self.inner.raw_window_handle()
    }
}

impl WgpuWindow {
    pub fn update(&mut self) -> Result<(), WindowError> {
        self.with_handler(|handler, window| {
            let result = window
                .inner
                .refresh_metrics()
                .map_err(anyhow::Error::from)
                .and_then(|()| handler.update(window));
            result.or_else(|error| handler.error(error))
        })
    }

    pub fn window_handle_provider(&self) -> Rc<dyn raw_window_handle::HasWindowHandle> {
        self.inner.window_handle_provider()
    }
}

// Retain native operations privately for future host needs.
#[allow(dead_code)]
impl WgpuWindow {
    fn show_system_menu(&mut self, x: i32, y: i32) -> Result<(), WindowError> {
        self.inner.show_system_menu(x, y)
    }

    fn get_pixel_density(&self) -> Result<f32, WindowError> {
        self.inner.get_pixel_density()
    }

    fn get_display_scale(&self) -> Result<f32, WindowError> {
        self.inner.get_display_scale()
    }

    fn get_display(&self) -> Result<u32, WindowError> {
        self.inner.get_display()
    }
    fn set_fullscreen_mode(&mut self, mode: Option<&WindowDisplayMode>) -> Result<(), WindowError> {
        self.inner.set_fullscreen_mode(mode)
    }
    fn get_fullscreen_mode(&self) -> Option<WindowDisplayMode> {
        self.inner.get_fullscreen_mode()
    }
    fn get_icc_profile(&self) -> Result<Vec<u8>, WindowError> {
        self.inner.get_icc_profile()
    }
    fn get_pixel_format(&self) -> Result<WindowPixelFormat, WindowError> {
        self.inner.get_pixel_format()
    }
    fn get_properties(&self) -> Result<WindowPropertiesId, WindowError> {
        self.inner.get_properties()
    }
    fn is_hidden(&self) -> bool {
        self.inner.is_hidden()
    }
    fn is_resizable(&self) -> bool {
        self.inner.is_resizable()
    }
    fn set_title(&mut self, title: &str) -> Result<(), WindowError> {
        self.inner.set_title(title)
    }
    fn get_title(&self) -> String {
        self.inner.get_title()
    }
    fn set_icon(&mut self, icon: &WindowSurfaceRef<'_>) -> Result<(), WindowError> {
        self.inner.set_icon(icon)
    }
    fn get_position(&self) -> Result<(i32, i32), WindowError> {
        self.inner.get_position()
    }
    fn get_size(&self) -> Result<(i32, i32), WindowError> {
        self.inner.get_size()
    }
    fn get_safe_area(&self) -> Result<WindowRect, WindowError> {
        self.inner.get_safe_area()
    }
    fn set_aspect_ratio(&mut self, min_aspect: f32, max_aspect: f32) -> Result<(), WindowError> {
        self.inner.set_aspect_ratio(min_aspect, max_aspect)
    }
    fn get_aspect_ratio(&self) -> Result<(f32, f32), WindowError> {
        self.inner.get_aspect_ratio()
    }
    fn get_borders_size(&self) -> Result<(i32, i32, i32, i32), WindowError> {
        self.inner.get_borders_size()
    }
    fn get_size_in_pixels(&self) -> Result<(i32, i32), WindowError> {
        self.inner.get_size_in_pixels()
    }
    fn set_minimum_size(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        self.inner.set_minimum_size(width, height)
    }
    fn get_minimum_size(&self) -> Result<(i32, i32), WindowError> {
        self.inner.get_minimum_size()
    }
    fn set_maximum_size(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        self.inner.set_maximum_size(width, height)
    }
    fn get_maximum_size(&self) -> Result<(i32, i32), WindowError> {
        self.inner.get_maximum_size()
    }
    fn set_resizable(&mut self, resizable: bool) -> Result<(), WindowError> {
        self.inner.set_resizable(resizable)
    }
    fn set_always_on_top(&mut self, on_top: bool) -> Result<(), WindowError> {
        self.inner.set_always_on_top(on_top)
    }
    fn set_fill_document(&mut self, fill: bool) -> Result<(), WindowError> {
        self.inner.set_fill_document(fill)
    }
    fn show(&mut self) -> Result<(), WindowError> {
        self.inner.show()
    }
    fn hide(&mut self) -> Result<(), WindowError> {
        self.inner.hide()
    }
    fn sync(&mut self) -> Result<(), WindowError> {
        self.inner.sync()
    }
    fn has_surface(&self) -> bool {
        self.inner.has_surface()
    }
    fn get_surface(&mut self) -> Result<WindowSurfaceRef<'_>, WindowError> {
        self.inner.get_surface()
    }
    fn set_surface_vsync(&mut self, vsync: i32) -> Result<(), WindowError> {
        self.inner.set_surface_vsync(vsync)
    }
    fn get_surface_vsync(&self) -> Result<i32, WindowError> {
        self.inner.get_surface_vsync()
    }
    fn update_surface(&mut self) -> Result<(), WindowError> {
        self.inner.update_surface()
    }
    fn update_surface_rects(&mut self, rects: &[WindowRect]) -> Result<(), WindowError> {
        self.inner.update_surface_rects(rects)
    }
    fn destroy_surface(&mut self) -> Result<(), WindowError> {
        self.inner.destroy_surface()
    }
    fn set_keyboard_grab(&mut self, grabbed: bool) -> Result<(), WindowError> {
        self.inner.set_keyboard_grab(grabbed)
    }
    fn set_mouse_grab(&mut self, grabbed: bool) -> Result<(), WindowError> {
        self.inner.set_mouse_grab(grabbed)
    }
    fn get_keyboard_grab(&self) -> bool {
        self.inner.get_keyboard_grab()
    }
    fn get_mouse_grab(&self) -> bool {
        self.inner.get_mouse_grab()
    }
    fn set_mouse_rect(&mut self, rect: Option<&WindowRect>) -> Result<(), WindowError> {
        self.inner.set_mouse_rect(rect)
    }
    fn get_mouse_rect(&self) -> Option<WindowRect> {
        self.inner.get_mouse_rect()
    }
    fn set_opacity(&mut self, opacity: f32) -> Result<(), WindowError> {
        self.inner.set_opacity(opacity)
    }
    fn get_opacity(&self) -> Result<f32, WindowError> {
        self.inner.get_opacity()
    }
    fn set_focusable(&mut self, focusable: bool) -> Result<(), WindowError> {
        self.inner.set_focusable(focusable)
    }
    fn set_shape(&mut self, shape: Option<&WindowSurfaceRef<'_>>) -> Result<(), WindowError> {
        self.inner.set_shape(shape)
    }
    fn flash(&mut self, operation: WindowFlashOperation) -> Result<(), WindowError> {
        self.inner.flash(operation)
    }
    fn set_progress_state(&mut self, state: WindowProgressState) -> Result<(), WindowError> {
        self.inner.set_progress_state(state)
    }
    fn get_progress_state(&self) -> Result<WindowProgressState, WindowError> {
        self.inner.get_progress_state()
    }
    fn set_progress_value(&mut self, value: f32) -> Result<(), WindowError> {
        self.inner.set_progress_value(value)
    }
    fn get_progress_value(&self) -> Result<f32, WindowError> {
        self.inner.get_progress_value()
    }
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, WindowError> {
        self.inner.display_handle()
    }
}

impl WgpuWindow {
    pub(super) fn initialize<H: WindowHandler<Self> + 'static>(
        inner: Self,
        handler: H,
    ) -> Result<Self, WindowError> {
        let mut window = inner;
        window.handler = Some(Box::new(handler));
        window.with_handler(|handler, window| {
            handler.init(window).or_else(|error| handler.error(error))
        })?;
        window.request_redraw()?;
        Ok(window)
    }

    fn with_handler<T>(
        &mut self,
        callback: impl FnOnce(&mut dyn WindowHandler<Self>, &mut Self) -> anyhow::Result<T>,
    ) -> Result<T, WindowError> {
        let mut handler = self
            .handler
            .take()
            .ok_or_else(|| WindowError::Nul("a window callback is already active".into()))?;
        let result = catch_unwind(AssertUnwindSafe(|| callback(handler.as_mut(), self)));
        if self.handler.is_none() {
            self.handler = Some(handler);
        } else if result.is_ok() {
            return Err(WindowError::Nul(
                "the callback replaced the active window".into(),
            ));
        }
        match result {
            Ok(result) => result.map_err(WindowError::Callback),
            Err(payload) => resume_unwind(payload),
        }
    }
}

/// Owns window configuration and a handler until build initializes the window.
/// Settings can be applied before or after handler(); no handler uses empty callbacks.
pub struct WgpuWindowBuilder<H = ()> {
    context: Option<Rc<sdl::SdlContext>>,
    title: String,
    position: (i32, i32),
    size: (i32, i32),
    resizable: bool,
    hidden: bool,
    handler: H,
}

impl<H> WgpuWindowBuilder<H> {
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
    pub fn handler<T>(self, handler: T) -> WgpuWindowBuilder<T> {
        WgpuWindowBuilder {
            context: self.context,
            title: self.title,
            position: self.position,
            size: self.size,
            resizable: self.resizable,
            hidden: self.hidden,
            handler,
        }
    }
}

impl<H: WindowHandler<WgpuWindow> + 'static> WgpuWindowBuilder<H> {
    pub fn build(self) -> Result<WgpuWindow, WindowError> {
        let native = match self.context {
            Some(context) => sdl::SdlInstance { context },
            None => sdl::init()?,
        };
        let inner = native.create_wgpu_window(
            &self.title,
            self.position,
            self.size,
            self.resizable,
            self.hidden,
        )?;
        WgpuWindow::initialize(
            WgpuWindow {
                handler: None,
                inner,
            },
            self.handler,
        )
    }
}

impl WgpuWindowBuilder {
    pub(crate) fn with_context(context: Rc<sdl::SdlContext>) -> Self {
        Self {
            context: Some(context),
            ..Self::new()
        }
    }

    fn new() -> Self {
        Self {
            context: None,
            title: String::from("cxstd"),
            position: (100, 100),
            size: (800, 600),
            resizable: false,
            hidden: false,
            handler: (),
        }
    }
}
