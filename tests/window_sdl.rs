use std::{
    thread,
    time::{Duration, Instant},
};

use cxstd::window::{WindowError, WindowEvent, WindowFlags, WindowId, sdl};

#[test]
fn initializes() -> Result<(), WindowError> {
    let _sdl_inst = sdl::init()?;
    Ok(())
}

#[test]
fn creates_window() -> Result<(), WindowError> {
    let sdl_inst = sdl::init()?;
    let _window = sdl_inst.create_window("cxstd window test", 320, 240, WindowFlags::HIDDEN)?;
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
