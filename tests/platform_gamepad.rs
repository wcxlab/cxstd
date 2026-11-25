#![cfg(not(target_vendor = "apple"))]

use cxstd::platform::PlatformEvent;
use cxstd::{GamepadAxis, GamepadButtonCode};
use sdl3_sys::joystick::{
    SDL_AttachVirtualJoystick, SDL_DetachVirtualJoystick, SDL_JOYSTICK_TYPE_GAMEPAD,
    SDL_VirtualJoystickDesc,
};

#[test]
fn receives_gamepad_connection_without_enumerating_devices() -> anyhow::Result<()> {
    cxstd::platform::init()?;
    let descriptor = SDL_VirtualJoystickDesc {
        r#type: SDL_JOYSTICK_TYPE_GAMEPAD.0 as u16,
        name: c"cxstd Virtual Gamepad".as_ptr(),
        naxes: 6,
        nbuttons: 15,
        ..Default::default()
    };
    let id = unsafe { SDL_AttachVirtualJoystick(&descriptor) };
    assert_ne!(id.0, 0, "failed to attach the virtual gamepad");

    let gamepad = std::iter::from_fn(cxstd::platform::poll_event)
        .find_map(|message| match message.event {
            PlatformEvent::GamepadConnected(event)
                if event.gamepad.id == "cxstd Virtual Gamepad" =>
            {
                Some(event.gamepad)
            }
            _ => None,
        })
        .expect("virtual gamepad connection event");
    let gamepad = cxstd::platform::gamepad()
        .get_gamepad(gamepad.index)
        .expect("connected gamepad");
    let buttons = gamepad.buttons();
    assert!(std::rc::Rc::ptr_eq(&buttons, &gamepad.buttons()));
    assert!(!buttons[GamepadButtonCode::South as usize].pressed);
    assert_eq!(
        gamepad.axis(GamepadAxis::LeftStickX),
        Some(gamepad.axes()[GamepadAxis::LeftStickX as usize])
    );

    assert!(unsafe { SDL_DetachVirtualJoystick(id) });
    while cxstd::platform::poll_event().is_some() {}
    assert!(!buttons[GamepadButtonCode::South as usize].pressed);
    Ok(())
}
