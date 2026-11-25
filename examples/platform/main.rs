//! Run with `cargo run --example platform`.
//! Open an 800x600 logical-pixel window and print received events.
//! Left-click within x=100..700, y=100..500 to enable IME; click outside to disable it.

use cxstd::KeyCode;
use cxstd::platform::{PlatformEvent, PlatformEventMessage};
use cxstd::platform::window::{Window, WindowHandlerTrait, WindowTrait};
use std::time::Duration;

#[derive(Default)]
struct MainWindow {
    text_input_active: bool,
}

impl WindowHandlerTrait<Window> for MainWindow {
    fn event(
        &mut self,
        window: &mut Window,
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
            PlatformEvent::Close => {
                if self.text_input_active {
                    window.stop_text_input()?;
                    self.text_input_active = false;
                }
                window.close()?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cxstd::platform::init()?;

    let mut window = cxstd::platform::create_window()
        .title("cxstd platform example")
        .size(800, 600)
        .resizable(true)
        .handler(MainWindow::default())
        .build()?;

    loop {
        while let Some(event) = cxstd::platform::poll_event() {
            if event.window_id == window.id() {
                window.handle_event(&event)?;
            } else {
                println!("{event:?}");
            }

            if matches!(event.event, PlatformEvent::Quit) {
                return Ok(());
            }
        }

        tokio::time::sleep(Duration::from_millis(8)).await;
    }
}
