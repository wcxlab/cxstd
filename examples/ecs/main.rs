//! Run with `cargo run --example ecs`.
//! AppCore and AppView each own an Ecs instance.
//! The host updates business state before updating and rendering views.
//! The first frame runs immediately, followed by one frame each second.
//! Systems use their own ECS data and resources without an external host argument.

mod app_core;
mod app_view;
use std::time::Duration;

use app_core::AppCore;
use app_view::AppView;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut app_core = AppCore::new()?;
    let mut app_view = AppView::new()?;
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        app_core.update()?;
        app_view.update(&app_core)?;
        app_view.render()?;
    }
}
