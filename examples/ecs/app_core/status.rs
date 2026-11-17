//! `StatusPlugin::default()` registers update timing and a shared asynchronous counter.

use cxstd::ecs::{Plugin, PluginContext, ResMut, Resource};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct StatusPlugin;

impl Plugin for StatusPlugin {
    type LoadParams = ();

    fn load(&mut self, ctx: PluginContext, _deps: ()) -> anyhow::Result<()> {
        ctx.add_resource(Status {
            current_time: Instant::now(),
            update_duration: Duration::ZERO,
            counter: Arc::new(AtomicU64::new(0)),
        });
        ctx.add_system("pre_update", begin_update_system);
        ctx.add_schedule("update");
        ctx.add_system("post_update", finish_update_system);
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct Status {
    /// Monotonic start of the current update, not a wall-clock timestamp.
    pub current_time: Instant,
    /// Execution time measured between pre_update and post_update.
    pub update_duration: Duration,

    counter: Arc<AtomicU64>,
}
impl Resource for Status {}

impl Status {
    pub fn counter(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    /// Cloned Status handles share the same counter, which changes after the delay.
    pub async fn increment_counter(&self) -> u64 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.counter.fetch_add(1, Ordering::Relaxed) + 1
    }
}

fn begin_update_system(_: cxstd::ecs::SystemContext, deps: ResMut<Status>) {
    let mut status = deps;
    status.current_time = Instant::now();
}

fn finish_update_system(_: cxstd::ecs::SystemContext, deps: ResMut<Status>) {
    let mut status = deps;
    status.update_duration = status.current_time.elapsed();
}
