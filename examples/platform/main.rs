//! Run with `cargo run --example platform`.
//! Open an 800x600 logical-pixel window and print received events.
//! Left-click within x=100..700, y=100..500 to enable IME; click outside to disable it.

use std::time::Duration;

use anyhow::Result;
use cxstd::KeyCode;
use cxstd::platform::Platform;
use cxstd::platform::{PlatformEvent, PlatformEventMessage};
use cxstd::window::{WgpuWindow, Window, WindowHandler};

#[derive(Default)]
struct MainWindow {
    text_input_active: bool,
}

impl WindowHandler<WgpuWindow> for MainWindow {
    fn window_event(
        &mut self,
        window: &mut WgpuWindow,
        event: &PlatformEventMessage,
    ) -> anyhow::Result<()> {
        println!("{event:?}");
        match &event.event {
            PlatformEvent::MouseDown(mouse) if mouse.button == KeyCode::MouseLeft => {
                let active = (100.0..700.0).contains(&mouse.client_x)
                    && (100.0..500.0).contains(&mouse.client_y);
                if active != self.text_input_active {
                    if active {
                        window.start_text_input()?;
                    } else {
                        window.stop_text_input()?;
                    }
                    self.text_input_active = active;
                }
            }
            PlatformEvent::Blur if self.text_input_active => {
                window.stop_text_input()?;
                self.text_input_active = false;
            }
            _ => {}
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut platform = Platform::new()?;
    platform.gamepad().get_gamepads()?;
    let mut window = platform
        .window()
        .wgpu_window()
        .title("cxstd platform example")
        .size(800, 600)
        .resizable(true)
        .handler(MainWindow::default())
        .build()?;
    let window_id = window.id();

    loop {
        while let Some(event) = platform.event().poll_event() {
            if event.window_id == window_id {
                window.window_event(&event)?;
            } else {
                println!("{event:?}");
            }

            match event.event {
                PlatformEvent::Quit => return Ok(()),
                PlatformEvent::Close if event.window_id == window_id => return Ok(()),
                _ => {}
            }
        }

        tokio::time::sleep(Duration::from_millis(8)).await;
    }
}
