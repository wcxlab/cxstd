use std::{
    cell::Cell,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use cxstd::window::{
    WgpuFrame, WgpuWindow, Window, WindowError, WindowEvent, WindowEventKind, WindowFlags,
    WindowHandler, WindowOptions,
    sdl::{self, SdlWgpuWindow},
};

#[derive(Default)]
struct HandlerState {
    initialized: Cell<bool>,
    updates: Cell<usize>,
    events: Cell<usize>,
    frames: Cell<usize>,
}

#[derive(Default)]
struct FirstWindow {
    state: Rc<HandlerState>,
}

impl WindowHandler<SdlWgpuWindow> for FirstWindow {
    fn init(&mut self, window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        assert_eq!(window.get_position()?, (100, 100));
        assert!(!self.state.initialized.replace(true));
        window.set_title("First window")?;
        window.set_clear_color(wgpu::Color::GREEN);
        Ok(())
    }

    fn update(&mut self, _window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        self.state.updates.set(self.state.updates.get() + 1);
        Ok(())
    }

    fn window_event(
        &mut self,
        window: &mut SdlWgpuWindow,
        event: &WindowEvent,
    ) -> Result<(), WindowError> {
        if matches!(
            event.kind,
            WindowEventKind::WindowShown
                | WindowEventKind::WindowExposed { .. }
                | WindowEventKind::WindowPixelSizeChanged { .. }
        ) {
            assert_eq!(event.window_id, window.get_id()?);
            self.state.events.set(self.state.events.get() + 1);
        }
        Ok(())
    }

    fn render(&mut self, window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        let frame = window.next_frame()?;
        let _view = frame.texture_view();
        assert_eq!(window.get_title(), "First window");
        Ok(())
    }

    fn frame_ready(&mut self, _window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        self.state.frames.set(self.state.frames.get() + 1);
        Ok(())
    }
}

#[derive(Default)]
struct SecondWindow {
    state: Rc<HandlerState>,
}

impl WindowHandler<SdlWgpuWindow> for SecondWindow {
    fn init(&mut self, window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        assert_eq!(window.get_position()?, (500, 100));
        assert!(!self.state.initialized.replace(true));
        window.set_title("Second window")?;
        window.set_clear_color(wgpu::Color::BLUE);
        Ok(())
    }

    fn update(&mut self, _window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        self.state.updates.set(self.state.updates.get() + 1);
        Ok(())
    }

    fn window_event(
        &mut self,
        window: &mut SdlWgpuWindow,
        event: &WindowEvent,
    ) -> Result<(), WindowError> {
        if matches!(
            event.kind,
            WindowEventKind::WindowShown
                | WindowEventKind::WindowExposed { .. }
                | WindowEventKind::WindowPixelSizeChanged { .. }
        ) {
            assert_eq!(event.window_id, window.get_id()?);
            self.state.events.set(self.state.events.get() + 1);
        }
        Ok(())
    }

    fn render(&mut self, window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        let frame = window.next_frame()?;
        let _view = frame.texture_view();
        assert_eq!(window.get_title(), "Second window");
        Ok(())
    }

    fn frame_ready(&mut self, _window: &mut SdlWgpuWindow) -> Result<(), WindowError> {
        self.state.frames.set(self.state.frames.get() + 1);
        Ok(())
    }
}

#[test]
fn creates_two_windows_with_independent_handlers() -> Result<(), WindowError> {
    let mut sdl_inst = sdl::init()?;
    let first_state = Rc::new(HandlerState::default());
    let second_state = Rc::new(HandlerState::default());
    let mut first = sdl_inst.create_wgpu_window(
        FirstWindow {
            state: Rc::clone(&first_state),
        },
        WindowOptions {
            x: 100,
            y: 100,
            title: "first",
            width: 320,
            height: 240,
            flags: WindowFlags::RESIZABLE,
        },
    )?;
    let mut second = sdl_inst.create_wgpu_window(
        SecondWindow {
            state: Rc::clone(&second_state),
        },
        WindowOptions {
            x: 500,
            y: 100,
            title: "second",
            width: 400,
            height: 300,
            flags: WindowFlags::RESIZABLE,
        },
    )?;
    let first_id = first.get_id()?;
    let second_id = second.get_id()?;
    assert_ne!(first_id, second_id);
    assert!(first_state.initialized.get());
    assert!(second_state.initialized.get());
    assert_eq!(first.get_title(), "First window");
    assert_eq!(second.get_title(), "Second window");
    assert_eq!(first.clear_color(), wgpu::Color::GREEN);
    assert_eq!(second.clear_color(), wgpu::Color::BLUE);

    let deadline = Instant::now() + Duration::from_secs(10);
    while first_state.frames.get() < 3
        || second_state.frames.get() < 3
        || first_state.events.get() == 0
        || second_state.events.get() == 0
    {
        assert!(Instant::now() < deadline, "two-window rendering timed out");
        while let Some(event) = sdl_inst.poll_event() {
            if event.window_id == first_id {
                first.window_event(&event)?;
            } else if event.window_id == second_id {
                second.window_event(&event)?;
            }
        }
        first.update()?;
        second.update()?;
        first.render()?;
        second.render()?;
        thread::sleep(Duration::from_millis(16));
    }
    assert!(first_state.updates.get() > 0);
    assert!(second_state.updates.get() > 0);
    assert!(first.next_frame().is_err());
    assert!(second.next_frame().is_err());
    println!(
        "Both windows presented frames: first={}, second={}",
        first_state.frames.get(),
        second_state.frames.get()
    );
    Ok(())
}
