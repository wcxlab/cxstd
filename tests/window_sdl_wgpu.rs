use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    thread,
    time::{Duration, Instant},
};

use cxstd::window::{
    WgpuFrame, WgpuWindow, Window, WindowError, WindowEvent, WindowFlags, WindowHandler,
    sdl::{self, SdlInstance, SdlWgpuWindow},
};

fn create_window(visible: bool) -> Result<(SdlInstance, SdlWgpuWindow), WindowError> {
    let sdl_inst = sdl::init()?;
    let flags = if visible {
        WindowFlags::RESIZABLE
    } else {
        WindowFlags::HIDDEN
    };
    let window = sdl_inst.create_wgpu_window("cxstd wgpu test", 320, 240, flags)?;
    Ok((sdl_inst, window))
}

#[derive(Default)]
struct Handler {
    calls: Vec<&'static str>,
    fail_render: bool,
    panic_render: bool,
}

impl WindowHandler<SdlWgpuWindow> for Handler {
    fn init(&mut self, window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        self.calls.push("init");
        assert!(window.next_frame().is_err());
        window.set_title("initialized")
    }

    fn update(&mut self, window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        self.calls.push("update");
        assert!(window.next_frame().is_err());
        Ok(())
    }

    fn window_event(
        &mut self,
        window: &mut SdlWgpuWindow,
        event: &WindowEvent,
    ) -> Result<(), WindowError> {
        self.calls.push("event");
        if let WindowEvent::WindowPixelSizeChanged { width, height, .. } = event {
            if *width > 0 && *height > 0 {
                assert_eq!(window.surface_config().width, *width as u32);
                assert_eq!(window.surface_config().height, *height as u32);
            }
        }
        Ok(())
    }

    fn render(&mut self, window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        self.calls.push("render");
        let mut frame = window.next_frame()?;
        assert!(window.next_frame().is_err());
        let view = frame.texture_view().clone();
        {
            let _pass = frame
                .command_encoder()
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("cxstd integration test pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
        }
        assert!(!self.panic_render, "render callback panic");
        if self.fail_render {
            return Err(WindowError::Nul("render callback failed".into()));
        }
        drop(frame);
        assert!(window.resize_surface(320, 240).is_err());
        Ok(())
    }

    fn frame_ready(&mut self, window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        self.calls.push("frame_ready");
        assert!(window.next_frame().is_err());
        Ok(())
    }

    fn error(&mut self, error: WindowError) -> Result<(), WindowError> {
        self.calls.push("error");
        Err(error)
    }
}

fn render_frame(
    sdl_inst: &mut SdlInstance,
    window: &mut SdlWgpuWindow,
    handler: &mut Handler,
) -> Result<(), WindowError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        while let Some(event) = sdl_inst.poll_event() {
            window.window_event(&event, handler)?;
        }
        if window.render(handler)? {
            return Ok(());
        }
        assert!(Instant::now() < deadline, "wgpu frame timed out");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn initializes_gpu_and_forwards_native_operations() -> Result<(), WindowError> {
    let (_sdl_inst, mut window) = create_window(false)?;
    let (width, height) = window.get_size_in_pixels()?;
    assert_eq!(window.surface_config().width, width as u32);
    assert_eq!(window.surface_config().height, height as u32);
    assert!(matches!(
        window.surface_config().format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
    ));
    let native: &mut dyn Window = &mut window;
    native.set_title("forwarded")?;
    assert_eq!(native.get_title(), "forwarded");
    assert!(native.get_flags().is_hidden());
    assert!(native.window_handle().is_ok());
    assert!(window.next_frame().is_err());
    Ok(())
}

#[test]
fn resizes_before_event_callback_and_suspends_zero_size() -> Result<(), WindowError> {
    let (_sdl_inst, mut window) = create_window(false)?;
    let mut handler = Handler::default();
    let window_id = window.get_id()?;
    window.window_event(
        &WindowEvent::WindowPixelSizeChanged {
            timestamp: 0,
            window_id,
            width: 400,
            height: 300,
        },
        &mut handler,
    )?;
    assert_eq!(handler.calls, ["event"]);
    assert_eq!(
        (
            window.surface_config().width,
            window.surface_config().height
        ),
        (400, 300)
    );
    window.resize_surface(0, 0)?;
    assert!(!window.render(&mut handler)?);
    assert_eq!(handler.calls, ["event"]);
    window.resize_surface(320, 240)?;
    assert_eq!(
        (
            window.surface_config().width,
            window.surface_config().height
        ),
        (320, 240)
    );
    let limit = window.device().limits().max_texture_dimension_2d;
    assert!(window.resize_surface(limit + 1, 240).is_err());
    assert!(
        window
            .window_event(
                &WindowEvent::WindowPixelSizeChanged {
                    timestamp: 0,
                    window_id,
                    width: -1,
                    height: 240,
                },
                &mut handler
            )
            .is_err()
    );
    assert_eq!(handler.calls.last(), Some(&"error"));
    Ok(())
}

#[test]
fn rejects_software_surface_operations() -> Result<(), WindowError> {
    let (_sdl_inst, mut window) = create_window(false)?;
    assert!(!window.has_surface());
    assert!(window.get_surface().is_err());
    assert!(window.set_surface_vsync(1).is_err());
    assert!(window.get_surface_vsync().is_err());
    assert!(window.update_surface().is_err());
    assert!(window.update_surface_rects(&[]).is_err());
    assert!(window.destroy_surface().is_err());
    Ok(())
}

#[test]
fn dispatches_callbacks_and_presents_a_frame() -> Result<(), WindowError> {
    let (mut sdl_inst, mut window) = create_window(true)?;
    let mut handler = Handler::default();
    window.init(&mut handler)?;
    assert_eq!(window.get_title(), "initialized");
    window.update(&mut handler)?;
    render_frame(&mut sdl_inst, &mut window, &mut handler)?;
    let lifecycle: Vec<_> = handler
        .calls
        .iter()
        .copied()
        .filter(|call| *call != "event")
        .collect();
    assert_eq!(lifecycle, ["init", "update", "render", "frame_ready"]);
    assert!(window.next_frame().is_err());
    Ok(())
}

#[test]
fn releases_frame_after_callback_error() -> Result<(), WindowError> {
    let (mut sdl_inst, mut window) = create_window(true)?;
    let mut handler = Handler {
        fail_render: true,
        ..Default::default()
    };
    let result = render_frame(&mut sdl_inst, &mut window, &mut handler);
    assert!(
        matches!(result, Err(WindowError::Nul(message)) if message == "render callback failed")
    );
    assert_eq!(
        handler
            .calls
            .iter()
            .filter(|call| **call == "error")
            .count(),
        1
    );
    assert!(!handler.calls.contains(&"frame_ready"));
    assert!(window.next_frame().is_err());
    handler.fail_render = false;
    render_frame(&mut sdl_inst, &mut window, &mut handler)?;
    assert_eq!(handler.calls.last(), Some(&"frame_ready"));
    Ok(())
}

#[test]
fn releases_frame_during_unwinding_and_retains_sdl_context() -> Result<(), WindowError> {
    let (mut sdl_inst, mut window) = create_window(true)?;
    let mut handler = Handler {
        panic_render: true,
        ..Default::default()
    };
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            render_frame(&mut sdl_inst, &mut window, &mut handler)
        }))
        .is_err()
    );
    assert_eq!(handler.calls.last(), Some(&"render"));
    assert!(window.next_frame().is_err());
    handler.panic_render = false;
    render_frame(&mut sdl_inst, &mut window, &mut handler)?;
    drop(sdl_inst);
    window.set_title("context retained")?;
    assert_eq!(window.get_title(), "context retained");
    drop(window);
    let _sdl_inst = sdl::init()?;
    Ok(())
}
