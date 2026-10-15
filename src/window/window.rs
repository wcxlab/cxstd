use super::{
    WindowDisplayHandle, WindowDisplayId, WindowDisplayMode, WindowError, WindowEvent, WindowFlags,
    WindowFlashOperation, WindowHandle, WindowHitTestResult, WindowId, WindowPixelFormat,
    WindowProgressState, WindowPropertiesId, WindowRect, WindowSurfaceRef, WindowSystemCursor,
};

pub trait Window {
    fn request_redraw(&mut self);

    fn request_close(&mut self);

    fn take_redraw_request(&mut self) -> bool;

    fn take_close_request(&mut self) -> bool;

    fn get_id(&self) -> Result<WindowId, WindowError>;

    fn get_display(&self) -> Result<WindowDisplayId, WindowError>;

    fn get_pixel_density(&self) -> Result<f32, WindowError>;

    fn get_display_scale(&self) -> Result<f32, WindowError>;

    fn set_fullscreen_mode(&mut self, mode: Option<&WindowDisplayMode>) -> Result<(), WindowError>;

    fn get_fullscreen_mode(&self) -> Option<WindowDisplayMode>;

    fn get_icc_profile(&self) -> Result<Vec<u8>, WindowError>;

    fn get_pixel_format(&self) -> Result<WindowPixelFormat, WindowError>;

    fn get_properties(&self) -> Result<WindowPropertiesId, WindowError>;

    fn get_flags(&self) -> WindowFlags;

    fn set_cursor(&mut self, cursor: WindowSystemCursor) -> Result<(), WindowError>;

    fn start_text_input(&mut self) -> Result<(), WindowError>;

    fn stop_text_input(&mut self) -> Result<(), WindowError>;

    fn set_title(&mut self, title: &str) -> Result<(), WindowError>;

    fn get_title(&self) -> String;

    fn set_icon(&mut self, icon: &WindowSurfaceRef<'_>) -> Result<(), WindowError>;

    fn set_position(&mut self, x: i32, y: i32) -> Result<(), WindowError>;

    fn get_position(&self) -> Result<(i32, i32), WindowError>;

    fn set_size(&mut self, width: i32, height: i32) -> Result<(), WindowError>;

    fn get_size(&self) -> Result<(i32, i32), WindowError>;

    fn get_safe_area(&self) -> Result<WindowRect, WindowError>;

    fn set_aspect_ratio(&mut self, min_aspect: f32, max_aspect: f32) -> Result<(), WindowError>;

    fn get_aspect_ratio(&self) -> Result<(f32, f32), WindowError>;

    fn get_borders_size(&self) -> Result<(i32, i32, i32, i32), WindowError>;

    fn get_size_in_pixels(&self) -> Result<(i32, i32), WindowError>;

    fn set_minimum_size(&mut self, width: i32, height: i32) -> Result<(), WindowError>;

    fn get_minimum_size(&self) -> Result<(i32, i32), WindowError>;

    fn set_maximum_size(&mut self, width: i32, height: i32) -> Result<(), WindowError>;

    fn get_maximum_size(&self) -> Result<(i32, i32), WindowError>;

    fn set_bordered(&mut self, bordered: bool) -> Result<(), WindowError>;

    fn set_resizable(&mut self, resizable: bool) -> Result<(), WindowError>;

    fn set_always_on_top(&mut self, on_top: bool) -> Result<(), WindowError>;

    fn set_fill_document(&mut self, fill: bool) -> Result<(), WindowError>;

    fn show(&mut self) -> Result<(), WindowError>;

    fn hide(&mut self) -> Result<(), WindowError>;

    fn raise(&mut self) -> Result<(), WindowError>;

    fn maximize(&mut self) -> Result<(), WindowError>;

    fn minimize(&mut self) -> Result<(), WindowError>;

    fn restore(&mut self) -> Result<(), WindowError>;

    fn set_fullscreen(&mut self, fullscreen: bool) -> Result<(), WindowError>;

    fn sync(&self) -> Result<(), WindowError>;

    fn has_surface(&self) -> bool;

    fn get_surface(&mut self) -> Result<WindowSurfaceRef<'_>, WindowError>;

    fn set_surface_vsync(&mut self, vsync: i32) -> Result<(), WindowError>;

    fn get_surface_vsync(&self) -> Result<i32, WindowError>;

    fn update_surface(&mut self) -> Result<(), WindowError>;

    fn update_surface_rects(&mut self, rects: &[WindowRect]) -> Result<(), WindowError>;

    fn destroy_surface(&mut self) -> Result<(), WindowError>;

    fn set_keyboard_grab(&mut self, grabbed: bool) -> Result<(), WindowError>;

    fn set_mouse_grab(&mut self, grabbed: bool) -> Result<(), WindowError>;

    fn get_keyboard_grab(&self) -> bool;

    fn get_mouse_grab(&self) -> bool;

    fn set_mouse_rect(&mut self, rect: Option<&WindowRect>) -> Result<(), WindowError>;

    fn get_mouse_rect(&self) -> Option<WindowRect>;

    fn set_opacity(&mut self, opacity: f32) -> Result<(), WindowError>;

    fn get_opacity(&self) -> Result<f32, WindowError>;

    fn set_focusable(&mut self, focusable: bool) -> Result<(), WindowError>;

    fn show_system_menu(&mut self, x: i32, y: i32) -> Result<(), WindowError>;

    fn set_hit_test(
        &mut self,
        hit_test: impl Fn(i32, i32) -> WindowHitTestResult + 'static,
    ) -> Result<(), WindowError>
    where
        Self: Sized;

    fn set_shape(&mut self, shape: Option<&WindowSurfaceRef<'_>>) -> Result<(), WindowError>;

    fn flash(&mut self, operation: WindowFlashOperation) -> Result<(), WindowError>;

    fn set_progress_state(&mut self, state: WindowProgressState) -> Result<(), WindowError>;

    fn get_progress_state(&self) -> Result<WindowProgressState, WindowError>;

    fn set_progress_value(&mut self, value: f32) -> Result<(), WindowError>;

    fn get_progress_value(&self) -> Result<f32, WindowError>;

    fn display_handle(&self) -> Result<WindowDisplayHandle<'_>, WindowError>;

    fn window_handle(&self) -> Result<WindowHandle<'_>, WindowError>;
}

/// Host callbacks shared by native and GPU windows.
/// SDL factories store the window and handler separately, and call `init` once.
pub trait WindowHandler<W: Window + ?Sized> {
    fn error(&mut self, error: WindowError) -> Result<(), WindowError> {
        Err(error)
    }

    fn init(&mut self, _window: &mut W) -> Result<(), WindowError> {
        Ok(())
    }

    fn update(&mut self, _window: &mut W) -> Result<(), WindowError> {
        Ok(())
    }

    fn render(&mut self, _window: &mut W) -> Result<(), WindowError> {
        Ok(())
    }

    fn window_event(&mut self, _window: &mut W, _event: &WindowEvent) -> Result<(), WindowError> {
        Ok(())
    }

    fn frame_ready(&mut self, _window: &mut W) -> Result<(), WindowError> {
        Ok(())
    }
}

#[cfg(feature = "wgpu")]
pub trait WgpuFrame {
    fn texture_view(&self) -> &::wgpu::TextureView;

    fn command_encoder(&mut self) -> &mut ::wgpu::CommandEncoder;
}

#[cfg(feature = "wgpu")]
pub trait WgpuWindow: Window {
    type Frame<'a>: WgpuFrame
    where
        Self: 'a;

    fn instance(&self) -> &::wgpu::Instance;

    fn adapter(&self) -> &::wgpu::Adapter;

    fn device(&self) -> &::wgpu::Device;

    fn queue(&self) -> &::wgpu::Queue;

    fn surface_config(&self) -> &::wgpu::SurfaceConfiguration;

    fn clear_color(&self) -> ::wgpu::Color;

    fn set_clear_color(&mut self, color: ::wgpu::Color);

    fn resize_surface(&mut self, width: u32, height: u32) -> Result<(), WindowError>;

    fn next_frame(&self) -> Result<Self::Frame<'_>, WindowError>;
}
