use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use cxstd::platform::{self, PlatformEvent, PlatformEventFlags, PlatformEventMessage};
use cxstd::window::{WgpuWindow, Window, WindowCursor, WindowError, WindowHandler, WindowId};

static WINDOW_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn render_frame(window: &mut WgpuWindow) -> Result<(), WindowError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        while platform::poll_event().is_some() {}
        if window.render()? {
            return Ok(());
        }
        assert!(Instant::now() < deadline, "frame presentation timed out");
        thread::sleep(Duration::from_millis(10));
    }
}

#[derive(Default)]
struct Handler {
    calls: Rc<RefCell<Vec<&'static str>>>,
}

impl WindowHandler<WgpuWindow> for Handler {
    fn init(&mut self, _window: &mut WgpuWindow) -> anyhow::Result<()> {
        self.calls.borrow_mut().push("init");
        Ok(())
    }

    fn update(&mut self, _window: &mut WgpuWindow) -> anyhow::Result<()> {
        self.calls.borrow_mut().push("update");
        Ok(())
    }

    fn window_event(
        &mut self,
        _window: &mut WgpuWindow,
        _event: &PlatformEventMessage,
    ) -> anyhow::Result<()> {
        self.calls.borrow_mut().push("event");
        Ok(())
    }

    fn render(&mut self, _window: &mut WgpuWindow) -> anyhow::Result<()> {
        self.calls.borrow_mut().push("render");
        Ok(())
    }

    fn frame_ready(&mut self, _window: &mut WgpuWindow) -> anyhow::Result<()> {
        self.calls.borrow_mut().push("frame_ready");
        Ok(())
    }
}

#[test]
fn initializes_on_build() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _window = WgpuWindow::builder().hidden(true).build()?;
    Ok(())
}

#[test]
fn window_retains_handler_after_panics_and_drops_it_before_native_resources()
-> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    struct OwnedHandler {
        calls: Rc<Cell<usize>>,
        panic_next: bool,
        native_alive_on_drop: Rc<Cell<bool>>,
        owner: Option<std::rc::Weak<dyn raw_window_handle::HasWindowHandle>>,
    }
    impl WindowHandler<WgpuWindow> for OwnedHandler {
        fn init(&mut self, window: &mut WgpuWindow) -> anyhow::Result<()> {
            self.owner = Some(Rc::downgrade(&window.window_handle_provider()));
            assert!(window.update().is_err());
            Ok(())
        }
        fn update(&mut self, window: &mut WgpuWindow) -> anyhow::Result<()> {
            self.calls.set(self.calls.get() + 1);
            assert!(window.update().is_err());
            if std::mem::replace(&mut self.panic_next, false) {
                panic!("expected update panic");
            }
            Ok(())
        }
    }
    impl Drop for OwnedHandler {
        fn drop(&mut self) {
            self.native_alive_on_drop.set(
                self.owner
                    .as_ref()
                    .and_then(std::rc::Weak::upgrade)
                    .is_some(),
            );
        }
    }
    let calls = Rc::new(Cell::new(0));
    let native_alive_on_drop = Rc::new(Cell::new(false));
    let mut window: WgpuWindow = WgpuWindow::builder()
        .hidden(true)
        .handler(OwnedHandler {
            calls: calls.clone(),
            panic_next: true,
            native_alive_on_drop: native_alive_on_drop.clone(),
            owner: None,
        })
        .build()?;
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| window.update())).is_err());
    window.update()?;
    assert_eq!(calls.get(), 2);
    assert!(!native_alive_on_drop.get());
    drop(window);
    assert!(native_alive_on_drop.get());
    Ok(())
}

#[test]
fn builder_owns_configuration_and_initializes_only_when_built() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    assert!(platform::poll_event().is_none());
    let title = String::from("owned builder title");
    let calls = Rc::new(RefCell::new(Vec::new()));
    let builder = WgpuWindow::builder()
        .handler(Handler {
            calls: calls.clone(),
        })
        .title(title.clone())
        .position(25, 35)
        .size(300, 200)
        .resizable(true)
        .resizable(false)
        .hidden(true);
    drop(title);
    assert!(calls.borrow().is_empty());
    assert!(platform::poll_event().is_none());
    let native = builder.build()?;
    assert_eq!(*calls.borrow(), ["init"]);
    assert!((native.inner_width() - 300.0).abs() <= 1.0 / native.device_pixel_ratio());
    assert!((native.inner_height() - 200.0).abs() <= 1.0 / native.device_pixel_ratio());
    drop(native);
    assert!(platform::poll_event().is_none());
    Ok(())
}

#[test]
fn windows_share_the_video_owner_and_reject_creation_on_another_thread() -> Result<(), WindowError>
{
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let first = WgpuWindow::builder().hidden(true).build()?;
    let second = WgpuWindow::builder().hidden(true).build()?;
    assert!(
        thread::spawn(|| WgpuWindow::builder().hidden(true).build().is_err())
            .join()
            .unwrap()
    );
    drop(first);
    assert!(second.inner_width() > 0.0);
    drop(second);
    assert!(platform::poll_event().is_none());
    let _reopened = WgpuWindow::builder().hidden(true).build()?;
    Ok(())
}

#[test]
fn creates_window() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let window: WgpuWindow = WgpuWindow::builder()
        .title("cxstd window test")
        .position(40, 60)
        .size(320, 240)
        .hidden(true)
        .handler(Handler::default())
        .build()?;
    let window_api: &dyn Window = &window;
    assert_ne!(window_api.id(), WindowId(0));
    assert!(!window_api.is_focused());
    assert!((window_api.inner_width() - 320.0).abs() <= 1.0 / window_api.device_pixel_ratio());
    assert!((window_api.inner_height() - 240.0).abs() <= 1.0 / window_api.device_pixel_ratio());
    assert_eq!(
        window_api.inner_physical_width(),
        window.surface_config().width
    );
    assert_eq!(
        window_api.inner_physical_height(),
        window.surface_config().height
    );
    Ok(())
}

#[test]
fn rejects_nul_title() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let invalid_title = WgpuWindow::builder()
        .title("invalid\0title")
        .position(0, 0)
        .size(320, 240)
        .hidden(true)
        .handler(Handler::default())
        .build();
    assert!(matches!(invalid_title, Err(WindowError::Nul(message)) if !message.is_empty()));
    Ok(())
}

#[test]
fn sets_system_cursors_and_controls_text_input() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut window = WgpuWindow::builder()
        .title("input")
        .position(0, 0)
        .size(320, 240)
        .hidden(true)
        .handler(Handler::default())
        .build()?;
    window.set_cursor(WindowCursor::Text)?;
    window.set_cursor(WindowCursor::Text)?;
    window.set_cursor(WindowCursor::None)?;
    window.set_cursor(WindowCursor::Text)?;
    window.set_cursor(WindowCursor::Default)?;
    window.start_text_input()?;
    window.stop_text_input()?;
    Ok(())
}

#[cfg(target_os = "windows")]
#[test]
fn handle_provider_keeps_the_native_window_and_context_alive() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let window = WgpuWindow::builder()
        .title("handle owner")
        .position(0, 0)
        .size(320, 240)
        .hidden(true)
        .handler(Handler::default())
        .build()?;
    let expected = window.raw_window_handle()?.as_raw();
    let provider = window.window_handle_provider();
    drop(window);
    assert_eq!(
        provider
            .window_handle()
            .map_err(|_| WindowError::Unknown)?
            .as_raw(),
        expected
    );
    drop(provider);
    Ok(())
}

#[cfg(target_os = "windows")]
#[test]
fn exposes_native_handles() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let window = WgpuWindow::builder()
        .title("native handles")
        .position(0, 0)
        .size(320, 240)
        .hidden(true)
        .handler(Handler::default())
        .build()?;
    assert!(matches!(
        window.raw_window_handle()?.as_raw(),
        raw_window_handle::RawWindowHandle::Win32(_)
    ));
    Ok(())
}

#[test]
fn processes_window_events() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut window = WgpuWindow::builder()
        .title("cxstd window test")
        .position(0, 0)
        .size(320, 240)
        .resizable(true)
        .handler(Handler::default())
        .build()?;

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "window redraw event timed out");
        match platform::poll_event() {
            Some(PlatformEventMessage {
                window_id,
                event: PlatformEvent::Redraw,
                ..
            }) => {
                assert_ne!(window_id, WindowId(0));
                break;
            }
            Some(_) => {}
            None => thread::sleep(Duration::from_millis(10)),
        }
    }

    window.resize_to(400, 300)?;
    loop {
        assert!(Instant::now() < deadline, "window resize event timed out");
        if let Some(message) = platform::poll_event() {
            if message.window_id != window.id() {
                continue;
            }
            window.window_event(&message)?;
            if let PlatformEvent::Resize(event) = message.event {
                let tolerance = 1.0 / window.device_pixel_ratio();
                if (event.inner_width - 400.0).abs() > tolerance
                    || (event.inner_height - 300.0).abs() > tolerance
                {
                    continue;
                }
                assert_eq!(event.inner_physical_width, window.inner_physical_width());
                assert_eq!(event.inner_physical_height, window.inner_physical_height());
                assert_eq!(event.inner_width, window.inner_width());
                assert_eq!(event.inner_height, window.inner_height());
                assert_eq!(event.inner_physical_width, window.surface_config().width);
                assert_eq!(event.inner_physical_height, window.surface_config().height);
                break;
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
    // SDL can emit both window-size and pixel-size notifications for one resize.
    // The second notification must not become another logical Resize event.
    for _ in 0..2 {
        while let Some(message) = platform::poll_event() {
            assert!(
                message.window_id != window.id()
                    || !matches!(message.event, PlatformEvent::Resize(_)),
                "unchanged logical dimensions emitted another resize"
            );
        }
    }
    window.resize_to(400, 300)?;
    for _ in 0..2 {
        while let Some(message) = platform::poll_event() {
            assert!(
                message.window_id != window.id()
                    || !matches!(message.event, PlatformEvent::Resize(_)),
                "requesting the same logical size emitted a resize"
            );
        }
    }
    drop(window);
    Ok(())
}

#[test]
fn reinitializes_after_window_drop() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let window = WgpuWindow::builder()
        .title("cxstd window lifetime")
        .position(0, 0)
        .size(320, 240)
        .hidden(true)
        .handler(Handler::default())
        .build()?;
    drop(window);

    let _window = WgpuWindow::builder()
        .title("cxstd reinitialized")
        .position(0, 0)
        .size(320, 240)
        .hidden(true)
        .handler(Handler::default())
        .build()?;

    Ok(())
}

#[test]
fn owns_and_dispatches_window_handler() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut window = WgpuWindow::builder()
        .title("handler")
        .position(0, 0)
        .size(320, 240)
        .hidden(false)
        .handler(Handler {
            calls: Rc::clone(&calls),
        })
        .build()?;
    assert_eq!(*calls.borrow(), ["init"]);
    window.update()?;
    let event = PlatformEventMessage {
        time_stamp: Duration::ZERO,
        window_id: window.id(),
        event: PlatformEvent::Redraw,
    };
    window.window_event(&event)?;
    render_frame(&mut window)?;
    assert_eq!(
        *calls.borrow(),
        ["init", "update", "event", "render", "frame_ready"]
    );
    Ok(())
}

#[test]
fn releases_handler_when_initialization_fails() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    struct FailingHandler {
        errors: Rc<Cell<usize>>,
        dropped: Rc<Cell<bool>>,
    }

    impl WindowHandler<WgpuWindow> for FailingHandler {
        fn init(&mut self, window: &mut WgpuWindow) -> anyhow::Result<()> {
            assert_ne!(window.id(), WindowId(0));
            anyhow::bail!("initialization failed")
        }

        fn error(&mut self, error: anyhow::Error) -> anyhow::Result<()> {
            self.errors.set(self.errors.get() + 1);
            Err(error)
        }
    }

    impl Drop for FailingHandler {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }

    let errors = Rc::new(Cell::new(0));
    let dropped = Rc::new(Cell::new(false));
    let result = WgpuWindow::builder()
        .title("failed handler")
        .position(0, 0)
        .size(320, 240)
        .hidden(true)
        .handler(FailingHandler {
            errors: Rc::clone(&errors),
            dropped: Rc::clone(&dropped),
        })
        .build();
    assert!(
        matches!(result, Err(WindowError::Callback(error)) if error.to_string() == "initialization failed")
    );
    assert_eq!(errors.get(), 1);
    assert!(dropped.get());
    Ok(())
}

fn queued_requests() -> Vec<(WindowId, PlatformEvent)> {
    let mut requests = Vec::new();
    // A poll sentinel can defer events pushed during the preceding poll pass.
    for _ in 0..2 {
        while let Some(event) = platform::poll_event() {
            if matches!(event.event, PlatformEvent::Redraw | PlatformEvent::Close) {
                requests.push((event.window_id, event.event));
            }
        }
    }
    requests
}

#[test]
fn queues_targeted_requests_without_echoing_dispatched_events() -> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut first = WgpuWindow::builder()
        .hidden(true)
        .handler(Handler {
            calls: calls.clone(),
        })
        .build()?;
    let mut second = WgpuWindow::builder().hidden(true).build()?;
    let first_id = first.id();
    let second_id = second.id();
    assert_eq!(
        queued_requests(),
        [
            (first_id, PlatformEvent::Redraw),
            (second_id, PlatformEvent::Redraw),
        ]
    );

    let native: &mut dyn Window = &mut first;
    native.request_redraw()?;
    native.request_redraw()?;
    second.request_close()?;
    first.request_close()?;
    let requests = queued_requests();
    assert_eq!(
        requests,
        [
            (first_id, PlatformEvent::Redraw),
            (first_id, PlatformEvent::Redraw),
            (second_id, PlatformEvent::Close),
            (first_id, PlatformEvent::Close),
        ]
    );
    for (window_id, kind) in requests {
        let event = PlatformEventMessage {
            time_stamp: Duration::ZERO,
            window_id,
            event: kind,
        };
        first.window_event(&event)?;
        second.window_event(&event)?;
    }
    assert_eq!(*calls.borrow(), ["init", "event", "event", "event"]);
    assert!(queued_requests().is_empty());
    first.request_redraw()?;
    drop(first);
    assert!(queued_requests().is_empty());
    assert!(second.inner_width() > 0.0);
    Ok(())
}

#[test]
fn callback_requests_are_queued_for_later_polls_and_skipped_frames_can_retry()
-> Result<(), WindowError> {
    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    struct RequestHandler {
        fail_render: Rc<Cell<bool>>,
    }
    impl WindowHandler<WgpuWindow> for RequestHandler {
        fn update(&mut self, window: &mut WgpuWindow) -> anyhow::Result<()> {
            window.request_close()?;
            Ok(window.request_redraw()?)
        }
        fn render(&mut self, window: &mut WgpuWindow) -> anyhow::Result<()> {
            if self.fail_render.get() {
                Err(WindowError::Unknown.into())
            } else {
                Ok(window.request_redraw()?)
            }
        }
        fn error(&mut self, _: anyhow::Error) -> anyhow::Result<()> {
            Ok(())
        }
    }
    let fail_render = Rc::new(Cell::new(false));
    let mut window = WgpuWindow::builder()
        .title("handler requests")
        .position(0, 0)
        .size(320, 240)
        .handler(RequestHandler {
            fail_render: fail_render.clone(),
        })
        .build()?;
    let id = window.id();
    let initial_requests = queued_requests();
    assert!(!initial_requests.is_empty());
    assert!(
        initial_requests
            .iter()
            .all(|request| *request == (id, PlatformEvent::Redraw))
    );
    window.update()?;
    assert_eq!(
        queued_requests(),
        [(id, PlatformEvent::Close), (id, PlatformEvent::Redraw),]
    );
    render_frame(&mut window)?;
    assert_eq!(queued_requests(), [(id, PlatformEvent::Redraw)]);
    fail_render.set(true);
    window.request_redraw()?;
    assert_eq!(queued_requests(), [(id, PlatformEvent::Redraw)]);
    if !window.render()? {
        window.request_redraw()?;
    }
    assert_eq!(queued_requests(), [(id, PlatformEvent::Redraw)]);
    Ok(())
}

#[test]
fn normalizes_composition_lifecycle_per_window() -> Result<(), WindowError> {
    use sdl3_sys::{events, video::SDL_WindowID};
    use std::ffi::CStr;

    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut first = WgpuWindow::builder().hidden(true).build()?;
    let second = WgpuWindow::builder().hidden(true).build()?;
    first.start_text_input()?;

    fn send(id: WindowId, text: &'static CStr, commit: bool) {
        let mut event = if commit {
            events::SDL_Event {
                text: events::SDL_TextInputEvent {
                    r#type: events::SDL_EVENT_TEXT_INPUT,
                    windowID: SDL_WindowID(id.0),
                    text: text.as_ptr(),
                    ..Default::default()
                },
            }
        } else {
            events::SDL_Event {
                edit: events::SDL_TextEditingEvent {
                    r#type: events::SDL_EVENT_TEXT_EDITING,
                    windowID: SDL_WindowID(id.0),
                    text: text.as_ptr(),
                    start: 1,
                    length: 0,
                    ..Default::default()
                },
            }
        };
        // Static strings remain valid until the native event has been polled.
        assert!(unsafe { events::SDL_PushEvent(&mut event) });
    }

    fn drain() -> Vec<(WindowId, &'static str, String)> {
        let mut result = Vec::new();
        // A native poll sentinel may separate events pushed between poll cycles.
        for _ in 0..2 {
            while let Some(message) = platform::poll_event() {
                let (name, text) = match message.event {
                    PlatformEvent::CompositionStart(event) => ("start", event.data),
                    PlatformEvent::CompositionUpdate(event) => {
                        assert_eq!(
                            (event.selection_start, event.selection_end),
                            (Some(1), Some(1))
                        );
                        ("update", event.data)
                    }
                    PlatformEvent::CompositionEnd(event) => ("end", event.data),
                    PlatformEvent::TextInput(event) => ("input", event.data),
                    _ => continue,
                };
                result.push((message.window_id, name, text));
            }
        }
        result
    }

    let first_id = first.id();
    let second_id = second.id();
    drain();
    send(first_id, c"ni", false);
    send(second_id, c"hao", false);
    send(first_id, c"nihao", false);
    assert_eq!(
        drain(),
        vec![
            (first_id, "start", "".into()),
            (first_id, "update", "ni".into()),
            (second_id, "start", "".into()),
            (second_id, "update", "hao".into()),
            (first_id, "update", "nihao".into()),
        ]
    );

    send(first_id, c"你好", true);
    send(first_id, c"", false);
    send(second_id, c"", false);
    send(second_id, c"", false);
    send(first_id, c"x", true);
    assert_eq!(
        drain(),
        vec![
            (first_id, "input", "你好".into()),
            (first_id, "end", "你好".into()),
            (second_id, "end", "".into()),
            (first_id, "input", "x".into()),
        ]
    );

    send(first_id, c"cancel", false);
    drain();
    first.stop_text_input()?;
    assert_eq!(drain(), vec![(first_id, "end", "".into())]);

    send(second_id, c"blur", false);
    drain();
    let mut blur = events::SDL_Event {
        window: events::SDL_WindowEvent {
            r#type: events::SDL_EVENT_WINDOW_FOCUS_LOST,
            windowID: SDL_WindowID(second_id.0),
            ..Default::default()
        },
    };
    assert!(unsafe { events::SDL_PushEvent(&mut blur) });
    assert_eq!(drain(), vec![(second_id, "end", "".into())]);
    Ok(())
}

#[test]
fn normalizes_web_event_payloads() -> Result<(), WindowError> {
    use sdl3_sys::{events, keycode, scancode, video::SDL_WindowID};

    let _guard = WINDOW_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let window = WgpuWindow::builder().hidden(true).build()?;
    let id = SDL_WindowID(window.id().0);
    for _ in 0..2 {
        while platform::poll_event().is_some() {}
    }

    fn push(mut event: events::SDL_Event) {
        assert!(unsafe { events::SDL_PushEvent(&mut event) });
    }
    fn drain() -> Vec<PlatformEventMessage> {
        let mut messages = Vec::new();
        for _ in 0..2 {
            while let Some(message) = platform::poll_event() {
                messages.push(message);
            }
        }
        messages
    }
    let stamp = 123_456_789;
    push(events::SDL_Event {
        key: events::SDL_KeyboardEvent {
            r#type: events::SDL_EVENT_KEY_DOWN,
            timestamp: stamp,
            windowID: id,
            scancode: scancode::SDL_SCANCODE_F13,
            key: keycode::SDLK_F13,
            r#mod: keycode::SDL_KMOD_CTRL | keycode::SDL_KMOD_CAPS,
            down: true,
            repeat: true,
            ..Default::default()
        },
    });
    push(events::SDL_Event {
        motion: events::SDL_MouseMotionEvent {
            r#type: events::SDL_EVENT_MOUSE_MOTION,
            timestamp: stamp,
            windowID: id,
            state: sdl3_sys::mouse::SDL_BUTTON_MMASK | sdl3_sys::mouse::SDL_BUTTON_RMASK,
            x: 40.0,
            y: 20.0,
            xrel: 2.0,
            yrel: -3.0,
            ..Default::default()
        },
    });
    // Release the middle button while the right button stays pressed.
    push(events::SDL_Event {
        button: events::SDL_MouseButtonEvent {
            r#type: events::SDL_EVENT_MOUSE_BUTTON_UP,
            timestamp: stamp,
            windowID: id,
            button: 2,
            down: false,
            clicks: 2,
            x: 40.0,
            y: 20.0,
            ..Default::default()
        },
    });
    for direction in [
        sdl3_sys::mouse::SDL_MOUSEWHEEL_NORMAL,
        sdl3_sys::mouse::SDL_MOUSEWHEEL_FLIPPED,
    ] {
        push(events::SDL_Event {
            wheel: events::SDL_MouseWheelEvent {
                r#type: events::SDL_EVENT_MOUSE_WHEEL,
                timestamp: stamp,
                windowID: id,
                x: 0.25,
                y: 1.5,
                direction,
                mouse_x: 40.0,
                mouse_y: 20.0,
                ..Default::default()
            },
        });
    }
    push(events::SDL_Event {
        window: events::SDL_WindowEvent {
            r#type: events::SDL_EVENT_WINDOW_MOUSE_LEAVE,
            timestamp: stamp,
            windowID: id,
            ..Default::default()
        },
    });
    push(events::SDL_Event {
        window: events::SDL_WindowEvent {
            r#type: events::SDL_EVENT_WINDOW_MOUSE_ENTER,
            timestamp: stamp,
            windowID: id,
            ..Default::default()
        },
    });
    let messages = drain();
    let enter = messages
        .iter()
        .find_map(|message| match &message.event {
            PlatformEvent::MouseEnter(mouse) => Some(mouse.clone()),
            _ => None,
        })
        .expect("expected mouseenter");
    let leave = messages
        .iter()
        .find_map(|message| match &message.event {
            PlatformEvent::MouseLeave(mouse) => Some(mouse.clone()),
            _ => None,
        })
        .expect("expected mouseleave");
    assert_eq!(
        (
            enter.screen_x,
            enter.screen_y,
            enter.client_x,
            enter.client_y
        ),
        (
            leave.screen_x,
            leave.screen_y,
            leave.client_x,
            leave.client_y
        ),
    );
    assert!(enter.right_button() && !enter.left_button() && !enter.middle_button());
    assert!(leave.right_button() && !leave.left_button() && !leave.middle_button());
    assert!(leave.ctrl_key() && leave.caps_lock());
    assert!(enter.ctrl_key() && enter.caps_lock());
    let messages: Vec<_> = messages
        .into_iter()
        .filter(|message| {
            matches!(
                message.event,
                PlatformEvent::KeyDown(_)
                    | PlatformEvent::MouseMove(_)
                    | PlatformEvent::MouseUp(_)
                    | PlatformEvent::Wheel(_)
            )
        })
        .collect();
    assert_eq!(messages.len(), 5);
    assert!(
        messages
            .iter()
            .all(|message| message.time_stamp == Duration::from_nanos(stamp))
    );
    let PlatformEvent::KeyDown(key) = &messages[0].event else {
        panic!("expected keydown")
    };
    assert_eq!(
        (&*key.key, key.code.as_str(), key.location as u32),
        ("F13", "F13", 0)
    );
    assert_eq!(key.code, cxstd::KeyCode::F13);
    assert_eq!(key.location, cxstd::KeyLocation::Standard);
    let keyboard_snapshot = key.clone();
    let modifier_bits = u32::from((keycode::SDL_KMOD_CTRL | keycode::SDL_KMOD_CAPS).0);
    assert_eq!(key.flags(), modifier_bits | (1 << 16));
    assert!(key.ctrl_key() && key.repeat() && key.caps_lock());
    assert!(!key.shift_key() && !key.alt_key() && !key.meta_key());
    assert!(!key.num_lock() && !key.scroll_lock() && !key.alt_graph());
    assert!(!key.is_composing());
    let PlatformEvent::MouseMove(mouse) = &messages[1].event else {
        panic!("expected mousemove")
    };
    let mouse_snapshot = mouse.clone();
    assert_eq!(
        mouse.flags(),
        modifier_bits
            | ((sdl3_sys::mouse::SDL_BUTTON_MMASK | sdl3_sys::mouse::SDL_BUTTON_RMASK).0 << 18),
    );
    assert!(mouse.middle_button() && mouse.right_button());
    assert!(!mouse.left_button() && !mouse.back_button() && !mouse.forward_button());
    assert_eq!(
        (leave.client_x, leave.client_y),
        (mouse.client_x, mouse.client_y)
    );
    assert!(mouse.ctrl_key() && mouse.caps_lock());
    assert!(!mouse.repeat() && !mouse.is_composing());
    let factor = f64::from(window.inner_width()) / f64::from(window.inner_physical_width());
    // Native density may differ from the effective device ratio on high-DPI platforms.
    assert!(mouse.client_x > 0.0 && mouse.client_y > 0.0 && factor > 0.0);
    assert_eq!(mouse.movement_x / mouse.client_x, 2.0 / 40.0);
    assert_eq!(mouse.movement_y / mouse.client_y, -3.0 / 20.0);
    let PlatformEvent::MouseUp(mouse) = &messages[2].event else {
        panic!("expected mouseup")
    };
    assert_eq!(
        (mouse.button, mouse.detail),
        (cxstd::KeyCode::MouseMiddle, 2)
    );
    assert_eq!(
        mouse.flags(),
        modifier_bits | (sdl3_sys::mouse::SDL_BUTTON_RMASK.0 << 18),
    );
    assert!(mouse.right_button() && !mouse.middle_button() && !mouse.left_button());
    for (index, sign) in [(3, 1.0), (4, -1.0)] {
        let PlatformEvent::Wheel(wheel) = &messages[index].event else {
            panic!("expected wheel")
        };
        assert_eq!(
            (wheel.delta_x, wheel.delta_y, wheel.delta_z),
            (0.25 * sign, -1.5 * sign, 0.0)
        );
        assert!(wheel.right_button() && !wheel.left_button() && !wheel.middle_button());
        assert!(wheel.caps_lock());
    }

    push(events::SDL_Event {
        button: events::SDL_MouseButtonEvent {
            r#type: events::SDL_EVENT_MOUSE_BUTTON_DOWN,
            timestamp: stamp,
            windowID: id,
            button: 1,
            down: true,
            clicks: 1,
            x: 40.0,
            y: 20.0,
            ..Default::default()
        },
    });
    push(events::SDL_Event {
        key: events::SDL_KeyboardEvent {
            r#type: events::SDL_EVENT_KEY_UP,
            timestamp: stamp,
            windowID: id,
            scancode: scancode::SDL_SCANCODE_RCTRL,
            key: keycode::SDLK_RCTRL,
            r#mod: keycode::SDL_KMOD_CAPS,
            ..Default::default()
        },
    });
    let direction_events: Vec<_> = drain()
        .into_iter()
        .filter(|message| {
            matches!(
                message.event,
                PlatformEvent::MouseDown(_) | PlatformEvent::KeyUp(_)
            )
        })
        .collect();
    assert_eq!(direction_events.len(), 2);
    let PlatformEvent::MouseDown(press) = &direction_events[0].event else {
        panic!("expected mousedown")
    };
    assert_eq!((press.button, press.detail), (cxstd::KeyCode::MouseLeft, 1));
    assert!(press.left_button() && press.right_button() && !press.middle_button());
    assert_eq!(
        (press.client_x, press.client_y),
        (mouse.client_x, mouse.client_y)
    );
    assert!(press.ctrl_key() && press.caps_lock());
    let PlatformEvent::KeyUp(release) = &direction_events[1].event else {
        panic!("expected keyup")
    };
    assert_eq!(release.key, "Control");
    assert_eq!(release.code, cxstd::KeyCode::ControlRight);
    assert_eq!(release.location, cxstd::KeyLocation::Right);
    assert!(release.caps_lock());
    assert!(!release.ctrl_key() && !release.repeat() && !release.is_composing());
    assert!(!release.shift_key() && !release.alt_key() && !release.meta_key());
    assert!(!release.num_lock() && !release.scroll_lock() && !release.alt_graph());

    push(events::SDL_Event {
        motion: events::SDL_MouseMotionEvent {
            r#type: events::SDL_EVENT_MOUSE_MOTION,
            timestamp: stamp,
            windowID: id,
            state: sdl3_sys::mouse::SDL_BUTTON_X1MASK | sdl3_sys::mouse::SDL_BUTTON_X2MASK,
            x: 40.0,
            y: 20.0,
            ..Default::default()
        },
    });
    let messages = drain();
    let side_buttons = messages
        .iter()
        .find_map(|message| match &message.event {
            PlatformEvent::MouseMove(mouse) => Some(mouse),
            _ => None,
        })
        .expect("expected side-button motion");
    assert!(side_buttons.back_button() && side_buttons.forward_button());
    assert!(!side_buttons.left_button());
    assert!(!side_buttons.middle_button() && !side_buttons.right_button());

    push(events::SDL_Event {
        edit: events::SDL_TextEditingEvent {
            r#type: events::SDL_EVENT_TEXT_EDITING,
            timestamp: stamp,
            windowID: id,
            text: c"中😀".as_ptr(),
            start: 1,
            length: 1,
            ..Default::default()
        },
    });
    push(events::SDL_Event {
        key: events::SDL_KeyboardEvent {
            r#type: events::SDL_EVENT_KEY_DOWN,
            timestamp: stamp,
            windowID: id,
            scancode: scancode::SDL_SCANCODE_RGUI,
            key: keycode::SDLK_RGUI,
            down: true,
            repeat: true,
            ..Default::default()
        },
    });
    push(events::SDL_Event {
        text: events::SDL_TextInputEvent {
            r#type: events::SDL_EVENT_TEXT_INPUT,
            timestamp: stamp,
            windowID: id,
            text: c"中😀".as_ptr(),
            ..Default::default()
        },
    });
    let messages = drain();
    assert_eq!(messages.len(), 5);
    let PlatformEvent::CompositionUpdate(edit) = &messages[1].event else {
        panic!("expected preedit")
    };
    let composition_snapshot = edit.clone();
    assert_eq!(
        (&*edit.data, edit.selection_start, edit.selection_end),
        ("中😀", Some(1), Some(3))
    );
    let PlatformEvent::TextInput(input) = &messages[3].event else {
        panic!("expected input")
    };
    assert_eq!(input.data, "中😀");
    let PlatformEvent::KeyDown(key) = &messages[2].event else {
        panic!("expected keydown")
    };
    assert_eq!(
        (
            &*key.key,
            key.code.as_str(),
            key.location as u32,
            key.is_composing()
        ),
        ("Meta", "MetaRight", 2, true)
    );
    assert!(key.repeat());
    assert!(matches!(
        messages[4].event,
        PlatformEvent::CompositionEnd(_)
    ));

    push(events::SDL_Event {
        text: events::SDL_TextInputEvent {
            r#type: events::SDL_EVENT_TEXT_INPUT,
            timestamp: stamp,
            windowID: id,
            text: c"a".as_ptr(),
            ..Default::default()
        },
    });
    let messages = drain();
    let PlatformEvent::TextInput(input) = &messages[0].event else {
        panic!("expected input")
    };
    assert_eq!(input.data, "a");
    for (scancode, key, modifiers, expected_key, expected_code, location) in [
        (
            scancode::SDL_SCANCODE_AC_SEARCH,
            keycode::SDLK_AC_SEARCH,
            keycode::SDL_KMOD_NONE,
            "BrowserSearch",
            "Unidentified",
            0,
        ),
        (
            scancode::SDL_SCANCODE_KP_EQUALS,
            keycode::SDLK_KP_EQUALS,
            keycode::SDL_KMOD_NONE,
            "=",
            "Unidentified",
            3,
        ),
        (
            scancode::SDL_SCANCODE_NONUSBACKSLASH,
            keycode::SDLK_BACKSLASH,
            keycode::SDL_KMOD_NONE,
            "\\",
            "IntlBackslash",
            0,
        ),
        (
            scancode::SDL_SCANCODE_HELP,
            keycode::SDLK_HELP,
            keycode::SDL_KMOD_NONE,
            "Help",
            "Help",
            0,
        ),
        (
            scancode::SDL_SCANCODE_KP_1,
            keycode::SDLK_KP_1,
            keycode::SDL_KMOD_NUM,
            "1",
            "Numpad1",
            3,
        ),
        (
            scancode::SDL_SCANCODE_KP_1,
            keycode::SDLK_KP_1,
            keycode::SDL_KMOD_NONE,
            "End",
            "Numpad1",
            3,
        ),
        (
            scancode::SDL_SCANCODE_RALT,
            keycode::SDLK_RALT,
            keycode::SDL_KMOD_MODE,
            "AltGraph",
            "AltRight",
            2,
        ),
        (
            scancode::SDL_SCANCODE_UNKNOWN,
            keycode::SDL_Keycode(u32::from('中')),
            keycode::SDL_KMOD_NONE,
            "中",
            "Unidentified",
            0,
        ),
    ] {
        push(events::SDL_Event {
            key: events::SDL_KeyboardEvent {
                r#type: events::SDL_EVENT_KEY_DOWN,
                timestamp: stamp,
                windowID: id,
                scancode,
                key,
                r#mod: modifiers,
                down: true,
                ..Default::default()
            },
        });
        let messages = drain();
        let PlatformEvent::KeyDown(key) = &messages[0].event else {
            panic!("expected keydown")
        };
        assert_eq!(
            (&*key.key, key.code.as_str(), key.location as u32),
            (expected_key, expected_code, location)
        );
        assert!(!key.is_composing());
    }
    let saved_position = (mouse_snapshot.client_x, mouse_snapshot.client_y);
    drop(window);
    assert!(keyboard_snapshot.ctrl_key() && keyboard_snapshot.repeat());
    assert!(keyboard_snapshot.caps_lock());
    assert!(mouse_snapshot.ctrl_key());
    assert!(mouse_snapshot.middle_button() && mouse_snapshot.right_button());
    assert_eq!(
        (mouse_snapshot.client_x, mouse_snapshot.client_y),
        saved_position
    );
    assert_eq!(
        (
            composition_snapshot.selection_start,
            composition_snapshot.selection_end
        ),
        (Some(1), Some(3))
    );
    Ok(())
}
