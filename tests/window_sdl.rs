use std::{
    cell::Cell,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use cxstd::window::{
    Window, WindowError, WindowEvent, WindowFlags, WindowHitTestResult, WindowId,
    WindowSystemCursor, sdl,
};

#[test]
fn initializes() -> Result<(), WindowError> {
    let _sdl_inst = sdl::init()?;
    Ok(())
}

#[test]
fn creates_window() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let window: sdl::SdlWindow =
        sdl_inst.create_window("cxstd window test", 320, 240, WindowFlags::HIDDEN)?;
    let window: &dyn Window = &window;
    assert_ne!(window.get_id()?, WindowId(0));
    assert_eq!(window.get_size()?, (320, 240));
    assert!(window.get_flags().is_hidden());
    assert!(window.get_pixel_density()? > 0.0);
    assert!(window.get_display_scale()? > 0.0);
    Ok(())
}

#[test]
fn rejects_nul_title() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let invalid_title = sdl_inst.create_window("invalid\0title", 320, 240, WindowFlags::HIDDEN);
    assert!(matches!(invalid_title, Err(WindowError::Nul(message)) if !message.is_empty()));
    Ok(())
}

#[test]
fn updates_title_and_rejects_nul_update() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let mut window = sdl_inst.create_window("original", 320, 240, WindowFlags::HIDDEN)?;
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
    let mut window = sdl_inst.create_window("input", 320, 240, WindowFlags::HIDDEN)?;
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
    let window = sdl_inst.create_window("handle owner", 320, 240, WindowFlags::HIDDEN)?;
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
            sdl_inst.create_window("unsupported", 320, 240, WindowFlags::HIDDEN | flags),
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
    let mut window = sdl_inst.create_window("hit test", 320, 240, WindowFlags::HIDDEN)?;
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
    let window = sdl_inst.create_window("native handles", 320, 240, WindowFlags::HIDDEN)?;
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
    let window = sdl_inst.create_window("cxstd window test", 320, 240, WindowFlags::RESIZABLE)?;

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "window shown event timed out");
        match sdl_inst.poll_event() {
            Some(WindowEvent::WindowShown { window_id, .. }) => {
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
    let window = sdl_inst.create_window("cxstd window lifetime", 320, 240, WindowFlags::HIDDEN)?;
    drop(sdl_inst);
    drop(window);

    let sdl_inst = sdl::init()?;
    let _window = sdl_inst.create_window("cxstd reinitialized", 320, 240, WindowFlags::HIDDEN)?;

    Ok(())
}
