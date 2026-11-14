use std::{
    cell::Cell,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use cxstd::platform::{self, PlatformEvent, PlatformEventMessage};
use cxstd::window::{WgpuWindow, Window, WindowError, WindowHandler};

static WINDOW_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Default)]
struct HandlerState {
    initialized: Cell<bool>,
    updates: Cell<usize>,
    events: Cell<usize>,
    frames: Cell<usize>,
    fail_render: Cell<bool>,
    panic_render: Cell<bool>,
    errors: Cell<usize>,
}

#[derive(Default)]
struct FirstWindow {
    state: Rc<HandlerState>,
}

impl WindowHandler<WgpuWindow> for FirstWindow {
    fn error(&mut self, _: anyhow::Error) -> anyhow::Result<()> {
        self.state.errors.set(self.state.errors.get() + 1);
        Ok(())
    }
    fn init(&mut self, window: &mut WgpuWindow) -> anyhow::Result<()> {
        assert!(!self.state.initialized.replace(true));
        window.set_clear_color(wgpu::Color::GREEN);
        Ok(())
    }

    fn update(&mut self, _window: &mut WgpuWindow) -> anyhow::Result<()> {
        self.state.updates.set(self.state.updates.get() + 1);
        Ok(())
    }

    fn window_event(
        &mut self,
        window: &mut WgpuWindow,
        event: &PlatformEventMessage,
    ) -> anyhow::Result<()> {
        if matches!(
            event.event,
            PlatformEvent::Redraw | PlatformEvent::Resize(_)
        ) {
            assert_eq!(event.window_id, window.id());
            self.state.events.set(self.state.events.get() + 1);
        }
        Ok(())
    }

    fn render(&mut self, window: &mut WgpuWindow) -> anyhow::Result<()> {
        let frame = window.next_frame()?;
        let _view = frame.texture_view();
        assert!(window.next_frame().is_err());
        assert!(!self.state.panic_render.get(), "expected render panic");
        if self.state.fail_render.get() {
            return Err(WindowError::Unknown.into());
        }
        Ok(())
    }

    fn frame_ready(&mut self, _window: &mut WgpuWindow) -> anyhow::Result<()> {
        self.state.frames.set(self.state.frames.get() + 1);
        Ok(())
    }
}

#[derive(Default)]
struct SecondWindow {
    state: Rc<HandlerState>,
}

impl WindowHandler<WgpuWindow> for SecondWindow {
    fn init(&mut self, window: &mut WgpuWindow) -> anyhow::Result<()> {
        assert!(!self.state.initialized.replace(true));
        window.set_clear_color(wgpu::Color::BLUE);
        Ok(())
    }

    fn update(&mut self, _window: &mut WgpuWindow) -> anyhow::Result<()> {
        self.state.updates.set(self.state.updates.get() + 1);
        Ok(())
    }

    fn window_event(
        &mut self,
        window: &mut WgpuWindow,
        event: &PlatformEventMessage,
    ) -> anyhow::Result<()> {
        if matches!(
            event.event,
            PlatformEvent::Redraw | PlatformEvent::Resize(_)
        ) {
            assert_eq!(event.window_id, window.id());
            self.state.events.set(self.state.events.get() + 1);
        }
        Ok(())
    }

    fn render(&mut self, window: &mut WgpuWindow) -> anyhow::Result<()> {
        let frame = window.next_frame()?;
        let _view = frame.texture_view();
        Ok(())
    }

    fn frame_ready(&mut self, _window: &mut WgpuWindow) -> anyhow::Result<()> {
        self.state.frames.set(self.state.frames.get() + 1);
        Ok(())
    }
}

#[test]
fn creates_two_windows_with_independent_handlers() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let first_state = Rc::new(HandlerState::default());
    let second_state = Rc::new(HandlerState::default());
    let mut first: WgpuWindow = WgpuWindow::builder()
        .title("first")
        .position(100, 100)
        .size(320, 240)
        .resizable(true)
        .handler(FirstWindow {
            state: Rc::clone(&first_state),
        })
        .build()?;
    let mut second: WgpuWindow = WgpuWindow::builder()
        .title("second")
        .position(500, 100)
        .size(400, 300)
        .resizable(true)
        .handler(SecondWindow {
            state: Rc::clone(&second_state),
        })
        .build()?;
    let first_id = first.id();
    let second_id = second.id();
    assert_ne!(first_id, second_id);
    assert!(first_state.initialized.get());
    assert!(second_state.initialized.get());
    assert_eq!(first.clear_color(), wgpu::Color::GREEN);
    assert_eq!(second.clear_color(), wgpu::Color::BLUE);

    let deadline = Instant::now() + Duration::from_secs(10);
    while first_state.frames.get() < 3
        || second_state.frames.get() < 3
        || first_state.events.get() == 0
        || second_state.events.get() == 0
    {
        assert!(Instant::now() < deadline, "two-window rendering timed out");
        while let Some(event) = platform::poll_event() {
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
    let frames = first_state.frames.get();
    first_state.fail_render.set(true);
    while first_state.errors.get() == 0 {
        assert!(Instant::now() < deadline, "render error callback timed out");
        assert!(!first.render()?);
    }
    assert!(first.next_frame().is_err());
    assert_eq!(first_state.frames.get(), frames);
    first_state.fail_render.set(false);
    first_state.panic_render.set(true);
    loop {
        assert!(Instant::now() < deadline, "render panic callback timed out");
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| first.render())).is_err() {
            break;
        }
    }
    assert!(first.next_frame().is_err());
    assert_eq!(first_state.frames.get(), frames);
    first_state.panic_render.set(false);
    while !first.render()? {
        assert!(Instant::now() < deadline, "render recovery timed out");
    }
    assert_eq!(first_state.frames.get(), frames + 1);
    println!(
        "Both windows presented frames: first={}, second={}",
        first_state.frames.get(),
        second_state.frames.get()
    );
    Ok(())
}
