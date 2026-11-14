use super::{Task, TaskAbortHandle, TaskExecutor, TaskId, TaskScope};
use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{runtime::Handle, time::Interval};

/// Tokio-backed executor exposed to async-compatible owner code.
#[derive(Clone)]
pub struct BackgroundExecutor {
    handle: Handle,
    closed: Arc<std::sync::atomic::AtomicBool>,
    scope: Option<TaskScope>,
}

/// Owner-thread executor for foreground futures.
#[derive(Clone)]
pub struct ForegroundExecutor {
    executor: TaskExecutor,
    scope: Option<TaskScope>,
}

impl ForegroundExecutor {
    pub fn spawn<F, O>(&self, future: F) -> Task<O>
    where
        F: Future<Output = O> + 'static,
        O: 'static,
    {
        self.executor
            .spawn_local_scoped(future, self.scope.clone())
            .expect("cannot spawn a foreground task on a closed owner executor or scope")
    }
}

impl ForegroundExecutor {
    pub(crate) fn new(executor: &TaskExecutor, scope: Option<TaskScope>) -> Self {
        Self {
            executor: executor.clone(),
            scope,
        }
    }
}

impl BackgroundExecutor {
    pub fn now(&self) -> Instant {
        Instant::now()
    }

    /// Returns a cancellable task that completes after the given duration.
    pub fn timer(&self, duration: Duration) -> Task<()> {
        self.spawn(async move { tokio::time::sleep(duration).await })
    }

    /// Create a repeating timer using the host Tokio runtime.
    pub fn interval(&self, period: Duration) -> Interval {
        let _entered = self.handle.enter();
        tokio::time::interval(period)
    }

    /// Spawn a `Send + 'static` future without creating another runtime.
    pub fn spawn<F, O>(&self, future: F) -> Task<O>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        if self.closed.load(std::sync::atomic::Ordering::Acquire)
            || self.scope.as_ref().is_some_and(TaskScope::is_closed)
        {
            panic!("cannot spawn a task on a closed owner executor");
        }

        let task_id = TaskId::next_untracked();
        let scope = self.scope.clone();
        let registration = ScopeRegistration { task_id, scope };
        // Register cancellation before the future can finish, including before
        // an immediately ready or panicking task drops its registration.
        let (start, ready) = tokio::sync::oneshot::channel();
        let task = self.handle.spawn(async move {
            let _registration = registration;
            let _ = ready.await;
            future.await
        });
        let abort_handle = task.abort_handle();
        if let Some(scope) = &self.scope {
            scope.register(task_id, TaskAbortHandle::Tokio(abort_handle.clone()));
        }
        let _ = start.send(());
        Task::from_join_handle(task_id, task, self.scope.clone())
    }
}

impl BackgroundExecutor {
    pub(crate) fn new(executor: &TaskExecutor) -> Self {
        Self {
            handle: executor.tokio_handle(),
            closed: executor.closed_handle(),
            scope: None,
        }
    }

    pub(crate) fn new_for_scope(executor: &TaskExecutor, scope: TaskScope) -> Self {
        Self {
            handle: executor.tokio_handle(),
            closed: executor.closed_handle(),
            scope: Some(scope),
        }
    }
}

struct ScopeRegistration {
    task_id: TaskId,
    scope: Option<TaskScope>,
}

impl Drop for ScopeRegistration {
    fn drop(&mut self) {
        if let Some(scope) = &self.scope {
            scope.unregister(self.task_id);
        }
    }
}
