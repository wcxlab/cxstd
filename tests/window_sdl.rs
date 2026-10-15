use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use cxstd::window::{
    Window, WindowError, WindowEvent, WindowEventKind, WindowFlags, WindowHandler,
    WindowHitTestResult, WindowId, WindowOptions, WindowSystemCursor, sdl,
};

#[derive(Default)]
struct Handler {
    calls: Rc<RefCell<Vec<&'static str>>>,
}

impl WindowHandler<sdl::SdlWindow> for Handler {
    fn init(&mut self, _window: &mut sdl::SdlWindow) -> Result<(), WindowError> {
        self.calls.borrow_mut().push("init");
        Ok(())
    }

    fn update(&mut self, _window: &mut sdl::SdlWindow) -> Result<(), WindowError> {
        self.calls.borrow_mut().push("update");
        Ok(())
    }

    fn window_event(
        &mut self,
        _window: &mut sdl::SdlWindow,
        _event: &WindowEvent,
    ) -> Result<(), WindowError> {
        self.calls.borrow_mut().push("event");
        Ok(())
    }

    fn render(&mut self, _window: &mut sdl::SdlWindow) -> Result<(), WindowError> {
        self.calls.borrow_mut().push("render");
        Ok(())
    }

    fn frame_ready(&mut self, _window: &mut sdl::SdlWindow) -> Result<(), WindowError> {
        self.calls.borrow_mut().push("frame_ready");
        Ok(())
    }
}

#[test]
fn initializes() -> Result<(), WindowError> {
    let _sdl_inst = sdl::init()?;
    Ok(())
}

#[test]
fn creates_window() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let window = sdl_inst.create_window(
        Handler::default(),
        WindowOptions {
            x: 40,
            y: 60,
            title: "cxstd window test",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    )?;
    let window: &dyn Window = &*window;
    assert_ne!(window.get_id()?, WindowId(0));
    assert_eq!(window.get_size()?, (320, 240));
    assert_eq!(window.get_position()?, (40, 60));
    assert!(window.get_flags().is_hidden());
    assert!(window.get_pixel_density()? > 0.0);
    assert!(window.get_display_scale()? > 0.0);
    Ok(())
}

#[test]
fn rejects_nul_title() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let invalid_title = sdl_inst.create_window(
        Handler::default(),
        WindowOptions {
            x: 0,
            y: 0,
            title: "invalid\0title",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    );
    assert!(matches!(invalid_title, Err(WindowError::Nul(message)) if !message.is_empty()));
    Ok(())
}

#[test]
fn updates_title_and_rejects_nul_update() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let mut window = sdl_inst.create_window(
        Handler::default(),
        WindowOptions {
            x: 0,
            y: 0,
            title: "original",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    )?;
    window.set_title("updated")?;
    assert_eq!(window.get_title(), "updated");
    assert!(matches!(
        window.set_title("invalid\0title"),
        Err(WindowError::Nul(_))
    ));
    assert_eq!(window.get_title(), "updated");
    Ok(())
}

#[test]
fn sets_system_cursors_and_controls_text_input() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let mut window = sdl_inst.create_window(
        Handler::default(),
        WindowOptions {
            x: 0,
            y: 0,
            title: "input",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    )?;
    window.set_cursor(WindowSystemCursor::Text)?;
    window.set_cursor(WindowSystemCursor::Text)?;
    window.set_cursor(WindowSystemCursor::Default)?;
    window.start_text_input()?;
    window.stop_text_input()?;
    Ok(())
}

#[cfg(target_os = "windows")]
#[test]
fn handle_provider_keeps_the_native_window_and_context_alive() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let window = sdl_inst.create_window(
        Handler::default(),
        WindowOptions {
            x: 0,
            y: 0,
            title: "handle owner",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    )?;
    let expected = window.window_handle()?.as_raw();
    let provider = window.window_handle_provider();
    drop(window);
    drop(sdl_inst);
    assert_eq!(
        provider
            .window_handle()
            .map_err(|_| WindowError::Unknown)?
            .as_raw(),
        expected
    );
    drop(provider);
    let _sdl_inst = sdl::init()?;
    Ok(())
}

#[test]
fn rejects_child_window_flags() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    for flags in [
        WindowFlags::MODAL,
        WindowFlags::TOOLTIP,
        WindowFlags::POPUP_MENU,
    ] {
        assert!(matches!(
                    sdl_inst.create_window(
        Handler::default(),
        WindowOptions { x: 0, y: 0, title: "unsupported", width: 320, height: 240, flags: WindowFlags::HIDDEN | flags },
        ),
                    Err(WindowError::Nul(message)) if message == "child windows are not supported"
                ));
    }
    Ok(())
}

#[test]
fn releases_replaced_and_destroyed_hit_test_callbacks() -> Result<(), WindowError> {
    struct DropCounter(Rc<Cell<usize>>);

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let sdl_inst = sdl::init()?;
    let mut window = sdl_inst.create_window(
        Handler::default(),
        WindowOptions {
            x: 0,
            y: 0,
            title: "hit test",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    )?;
    let dropped = Rc::new(Cell::new(0));
    for expected in 0..2 {
        let counter = DropCounter(Rc::clone(&dropped));
        let _ = window.set_hit_test(move |_x, _y| {
            let _counter = &counter;
            WindowHitTestResult::Normal
        });
        assert_eq!(dropped.get(), expected);
    }
    drop(window);
    assert_eq!(dropped.get(), 2);
    Ok(())
}

#[cfg(target_os = "windows")]
#[test]
fn exposes_native_handles() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let window = sdl_inst.create_window(
        Handler::default(),
        WindowOptions {
            x: 0,
            y: 0,
            title: "native handles",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    )?;
    assert!(matches!(
        window.display_handle()?.as_raw(),
        raw_window_handle::RawDisplayHandle::Windows(_)
    ));
    assert!(matches!(
        window.window_handle()?.as_raw(),
        raw_window_handle::RawWindowHandle::Win32(_)
    ));
    Ok(())
}

#[test]
fn processes_window_events() -> Result<(), WindowError> {
    let mut sdl_inst = sdl::init()?;
    let window = sdl_inst.create_window(
        Handler::default(),
        WindowOptions {
            x: 0,
            y: 0,
            title: "cxstd window test",
            width: 320,
            height: 240,
            flags: WindowFlags::RESIZABLE,
        },
    )?;

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "window shown event timed out");
        match sdl_inst.poll_event() {
            Some(WindowEvent {
                window_id,
                kind: WindowEventKind::WindowShown,
                ..
            }) => {
                assert_ne!(window_id, WindowId(0));
                break;
            }
            Some(_) => {}
            None => thread::sleep(Duration::from_millis(10)),
        }
    }

    drop(window);
    Ok(())
}

#[test]
fn reinitializes_after_window_drop() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let window = sdl_inst.create_window(
        Handler::default(),
        WindowOptions {
            x: 0,
            y: 0,
            title: "cxstd window lifetime",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    )?;
    drop(sdl_inst);
    drop(window);

    let sdl_inst = sdl::init()?;
    let _window = sdl_inst.create_window(
        Handler::default(),
        WindowOptions {
            x: 0,
            y: 0,
            title: "cxstd reinitialized",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    )?;

    Ok(())
}

#[test]
fn owns_and_dispatches_native_handler() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut window = sdl_inst.create_window(
        Handler {
            calls: Rc::clone(&calls),
        },
        WindowOptions {
            x: 0,
            y: 0,
            title: "handler",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    )?;
    assert_eq!(*calls.borrow(), ["init"]);
    window.update()?;
    let event = WindowEvent {
        timestamp: 0,
        window_id: window.get_id()?,
        kind: WindowEventKind::WindowExposed { live_resize: false },
    };
    window.window_event(&event)?;
    assert!(window.render()?);
    assert_eq!(
        *calls.borrow(),
        ["init", "update", "event", "render", "frame_ready"]
    );
    Ok(())
}

#[test]
fn releases_handler_when_initialization_fails() -> Result<(), WindowError> {
    struct FailingHandler {
        errors: Rc<Cell<usize>>,
        dropped: Rc<Cell<bool>>,
    }

    impl WindowHandler<sdl::SdlWindow> for FailingHandler {
        fn init(&mut self, window: &mut sdl::SdlWindow) -> Result<(), WindowError> {
            assert_ne!(window.get_id()?, WindowId(0));
            Err(WindowError::Nul("initialization failed".into()))
        }

        fn error(&mut self, error: WindowError) -> Result<(), WindowError> {
            self.errors.set(self.errors.get() + 1);
            Err(error)
        }
    }

    impl Drop for FailingHandler {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }

    let sdl_inst = sdl::init()?;
    let errors = Rc::new(Cell::new(0));
    let dropped = Rc::new(Cell::new(false));
    let result = sdl_inst.create_window(
        FailingHandler {
            errors: Rc::clone(&errors),
            dropped: Rc::clone(&dropped),
        },
        WindowOptions {
            x: 0,
            y: 0,
            title: "failed handler",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    );
    assert!(matches!(result, Err(WindowError::Nul(message)) if message == "initialization failed"));
    assert_eq!(errors.get(), 1);
    assert!(dropped.get());
    Ok(())
}

#[test]
fn merges_requests_and_routes_only_matching_window_events() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut window = sdl_inst.create_window(
        Handler {
            calls: Rc::clone(&calls),
        },
        WindowOptions {
            x: 0,
            y: 0,
            title: "requests",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    )?;
    let id = window.get_id()?;
    let native: &mut dyn Window = &mut *window;
    assert!(native.take_redraw_request());
    assert!(!native.take_redraw_request());
    assert!(!native.take_close_request());
    native.request_redraw();
    native.request_redraw();
    native.request_close();
    native.request_close();
    assert!(native.take_redraw_request());
    assert!(!native.take_redraw_request());
    assert!(native.take_close_request());
    assert!(!native.take_close_request());

    let mut event = WindowEvent {
        timestamp: 0,
        window_id: WindowId(0),
        kind: WindowEventKind::WindowCloseRequested,
    };
    window.window_event(&event)?;
    assert!(!window.take_close_request());
    assert!(!window.take_redraw_request());
    assert_eq!(*calls.borrow(), ["init"]);
    event.window_id = id;
    window.window_event(&event)?;
    assert!(window.take_close_request());
    assert!(!window.take_close_request());
    event.kind = WindowEventKind::WindowExposed { live_resize: false };
    window.window_event(&event)?;
    assert!(window.take_redraw_request());
    assert!(!window.take_redraw_request());
    assert_eq!(*calls.borrow(), ["init", "event", "event"]);
    Ok(())
}

#[test]
fn handler_requests_survive_rendering_and_can_retry_a_skipped_frame() -> Result<(), WindowError> {
    #[derive(Default)]
    struct RequestHandler {
        fail_render: Rc<Cell<bool>>,
    }

    impl WindowHandler<sdl::SdlWindow> for RequestHandler {
        fn update(&mut self, window: &mut sdl::SdlWindow) -> Result<(), WindowError> {
            window.request_close();
            window.request_redraw();
            Ok(())
        }

        fn render(&mut self, window: &mut sdl::SdlWindow) -> Result<(), WindowError> {
            if self.fail_render.get() {
                Err(WindowError::Unknown)
            } else {
                window.request_redraw();
                Ok(())
            }
        }

        fn error(&mut self, _: WindowError) -> Result<(), WindowError> {
            Ok(())
        }
    }

    let sdl_inst = sdl::init()?;
    let fail_render = Rc::new(Cell::new(false));
    let mut window = sdl_inst.create_window(
        RequestHandler {
            fail_render: Rc::clone(&fail_render),
        },
        WindowOptions {
            x: 0,
            y: 0,
            title: "handler requests",
            width: 320,
            height: 240,
            flags: WindowFlags::HIDDEN,
        },
    )?;
    assert!(window.take_redraw_request());
    window.update()?;
    assert!(window.take_close_request());
    assert!(!window.take_close_request());
    assert!(window.take_redraw_request());
    assert!(window.render()?);
    assert!(window.take_redraw_request());
    assert!(!window.take_redraw_request());

    fail_render.set(true);
    window.request_redraw();
    assert!(window.take_redraw_request());
    if !window.render()? {
        window.request_redraw();
    }
    assert!(window.take_redraw_request());
    assert!(!window.take_redraw_request());
    Ok(())
}
