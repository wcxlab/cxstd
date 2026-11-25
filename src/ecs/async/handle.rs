//! Task handles and cancellation scopes independent of UI contexts and elements.
//!
//! [`Task`] represents an awaitable Tokio task, a foreground future driven by
//! its owning executor, or callback-driven work. Awaitable handles cancel on drop
//! unless detached; callback-driven work remains owned by its executor and can
//! be aborted explicitly. Completion observers run through the owning executor.
//!
//! [`TaskScope`] groups asynchronous registrations for a cancellation scope,
//! aborting them when the scope closes and rejecting late registrations. It
//! contains cancellation state and task registrations.
//!
//! This module does not create a Tokio runtime or poll foreground futures.
//! The host initializes Tokio; TaskExecutor supplies polling and completion callbacks.

use futures::{channel::oneshot, future::AbortHandle as LocalAbortHandle};
use std::{
    any::Any,
    collections::HashMap,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::Poll,
};
use tokio::task::AbortHandle;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TaskId(u64);

pub struct Task<T = ()> {
    state: TaskState<T>,
    marker: PhantomData<fn() -> T>,
}

/// A reference to callback-driven work owned by the ECS executor.
/// Cloning or dropping the reference does not cancel the task. Results are
/// delivered to its completion callback; call `abort` to cancel explicitly.
#[derive(Clone)]
pub struct TaskRef {
    id: TaskId,
    abort_handle: TaskAbortHandle,
    canceled: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
}

pub(crate) struct TaskCompletion(pub(crate) Arc<AtomicBool>);

impl Drop for TaskCompletion {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

impl TaskRef {
    pub(crate) fn new<T>(
        task: Task<T>,
        canceled: Arc<AtomicBool>,
        finished: Arc<AtomicBool>,
    ) -> Self {
        Self {
            id: task.id(),
            abort_handle: task.control().abort_handle.clone(),
            canceled,
            finished,
        }
    }

    pub fn id(&self) -> TaskId {
        self.id
    }

    /// Cancel the future and suppress a completion still waiting at a barrier.
    pub fn abort(&self) {
        self.canceled.store(true, Ordering::Release);
        self.abort_handle.abort();
    }

    /// Whether completion has been handled or the operation has been canceled.
    /// A result waiting for its ECS callback is still in progress.
    pub fn is_finished(&self) -> bool {
        self.canceled.load(Ordering::Acquire) || self.finished.load(Ordering::Acquire)
    }
}

struct TaskControl {
    id: TaskId,
    abort_handle: TaskAbortHandle,
    scope: Option<TaskScope>,
    detached: bool,
    completion_callbacks: Arc<Mutex<Vec<Box<dyn FnOnce(&dyn Any) + Send>>>>,
}

#[derive(Clone)]
pub enum TaskAbortHandle {
    Tokio(AbortHandle),
    Local {
        handle: LocalAbortHandle,
        finished: Arc<AtomicBool>,
    },
}

enum TaskState<T> {
    Tracked(TaskControl),
    Awaitable {
        handle: tokio::task::JoinHandle<T>,
        control: TaskControl,
    },
    LocalAwaitable {
        receiver: oneshot::Receiver<T>,
        control: TaskControl,
    },
}

impl TaskId {
    /// Allocate from one process-wide sequence, including across executors
    /// that share a cancellation scope.
    #[doc(hidden)]
    pub fn next_untracked() -> Self {
        Self(NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl<T> Task<T> {
    /// Build an awaitable handle for a foreground future polled by its owner.
    #[doc(hidden)]
    pub fn from_local_receiver(
        id: TaskId,
        receiver: oneshot::Receiver<T>,
        abort_handle: TaskAbortHandle,
        scope: Option<TaskScope>,
    ) -> Self {
        Self {
            state: TaskState::LocalAwaitable {
                receiver,
                control: TaskControl::new(id, abort_handle, scope, Default::default()),
            },
            marker: PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn from_parts(
        id: TaskId,
        abort_handle: TaskAbortHandle,
        scope: Option<TaskScope>,
        completion_callbacks: Arc<Mutex<Vec<Box<dyn FnOnce(&dyn Any) + Send>>>>,
    ) -> Self {
        Self {
            state: TaskState::Tracked(TaskControl::new(
                id,
                abort_handle,
                scope,
                completion_callbacks,
            )),
            marker: PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn from_join_handle(
        id: TaskId,
        handle: tokio::task::JoinHandle<T>,
        scope: Option<TaskScope>,
    ) -> Self {
        let abort_handle = TaskAbortHandle::Tokio(handle.abort_handle());
        Self {
            state: TaskState::Awaitable {
                handle,
                control: TaskControl::new(id, abort_handle, scope, Default::default()),
            },
            marker: PhantomData,
        }
    }

    pub fn id(&self) -> TaskId {
        self.control().id
    }

    pub fn abort(&self) {
        self.control().abort();
    }

    /// Returns whether the task has finished running.
    pub fn is_finished(&self) -> bool {
        self.control().is_finished()
    }

    /// Detach a task so dropping the handle does not cancel it.
    pub fn detach(mut self) {
        self.control_mut().detached = true;
    }

    #[doc(hidden)]
    pub fn on_completion(&mut self, callback: impl FnOnce(&dyn Any) + Send + 'static) {
        self.control()
            .completion_callbacks
            .lock()
            .unwrap()
            .push(Box::new(callback));
    }

    #[doc(hidden)]
    pub fn is_callback_driven(&self) -> bool {
        matches!(self.state, TaskState::Tracked(_))
    }
}

impl<T> Task<T> {
    fn control(&self) -> &TaskControl {
        match &self.state {
            TaskState::Tracked(control) => control,
            TaskState::Awaitable { control, .. } | TaskState::LocalAwaitable { control, .. } => {
                control
            }
        }
    }

    fn control_mut(&mut self) -> &mut TaskControl {
        match &mut self.state {
            TaskState::Tracked(control) => control,
            TaskState::Awaitable { control, .. } | TaskState::LocalAwaitable { control, .. } => {
                control
            }
        }
    }
}

impl<T: 'static> Future for Task<T> {
    type Output = T;

    fn poll(
        mut self: Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.as_mut().get_mut();
        match &mut this.state {
            TaskState::Awaitable { handle, control } => match Pin::new(handle).poll(context) {
                std::task::Poll::Ready(Ok(output)) => {
                    control.notify_completion(&output);
                    std::task::Poll::Ready(output)
                }
                std::task::Poll::Ready(Err(error)) => panic!("async task failed: {error}"),
                std::task::Poll::Pending => std::task::Poll::Pending,
            },
            TaskState::LocalAwaitable { receiver, control } => {
                match Pin::new(receiver).poll(context) {
                    Poll::Ready(Ok(output)) => {
                        control.notify_completion(&output);
                        Poll::Ready(output)
                    }
                    Poll::Ready(Err(_)) => panic!("async local task was cancelled or failed"),
                    Poll::Pending => Poll::Pending,
                }
            }
            TaskState::Tracked(_) => {
                panic!("this task is callback-driven and cannot be awaited")
            }
        }
    }
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        match &self.state {
            TaskState::Awaitable { handle, control } if !control.detached => {
                handle.abort();
                control.unregister();
            }
            TaskState::LocalAwaitable { control, .. } if !control.detached => {
                control.abort_handle.abort();
                control.unregister();
            }
            _ => {}
        }
    }
}

impl TaskControl {
    fn new(
        id: TaskId,
        abort_handle: TaskAbortHandle,
        scope: Option<TaskScope>,
        completion_callbacks: Arc<Mutex<Vec<Box<dyn FnOnce(&dyn Any) + Send>>>>,
    ) -> Self {
        Self {
            id,
            abort_handle,
            scope,
            detached: false,
            completion_callbacks,
        }
    }

    fn notify_completion(&self, output: &dyn Any) {
        let callbacks = std::mem::take(&mut *self.completion_callbacks.lock().unwrap());
        for callback in callbacks {
            callback(output);
        }
    }

    fn abort(&self) {
        self.abort_handle.abort();
        self.unregister();
    }

    fn is_finished(&self) -> bool {
        self.abort_handle.is_finished()
    }

    fn unregister(&self) {
        if let Some(scope) = &self.scope {
            scope.unregister(self.id);
        }
    }
}

impl TaskAbortHandle {
    fn abort(&self) {
        match self {
            Self::Tokio(handle) => handle.abort(),
            Self::Local { handle, .. } => handle.abort(),
        }
    }

    fn is_finished(&self) -> bool {
        match self {
            Self::Tokio(handle) => handle.is_finished(),
            Self::Local { finished, .. } => finished.load(Ordering::Acquire),
        }
    }
}

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SCOPE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, thiserror::Error)]
#[error("asynchronous operation is closed")]
pub struct AsyncClosed;

/// Lifecycle state shared by a cancellation scope and its asynchronous contexts.
#[derive(Clone)]
pub struct TaskScope {
    id: u64,
    closed: Arc<AtomicBool>,
    tasks: Arc<Mutex<HashMap<TaskId, TaskAbortHandle>>>,
}

impl TaskScope {
    pub fn new() -> Self {
        Self {
            id: NEXT_SCOPE_ID.fetch_add(1, Ordering::Relaxed),
            closed: Arc::new(AtomicBool::new(false)),
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        for (_, task) in self.tasks.lock().unwrap().drain() {
            task.abort();
        }
    }

    pub fn register(&self, task_id: TaskId, abort_handle: TaskAbortHandle) {
        let mut tasks = self.tasks.lock().unwrap();
        if self.is_closed() {
            abort_handle.abort();
        } else {
            tasks.insert(task_id, abort_handle);
        }
    }

    pub fn unregister(&self, task_id: TaskId) {
        self.tasks.lock().unwrap().remove(&task_id);
    }

    #[doc(hidden)]
    pub fn task_count(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }
}

impl Default for TaskScope {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_task(scope: &TaskScope) -> (Task<usize>, LocalAbortHandle) {
        let id = TaskId::next_untracked();
        let (handle, _) = LocalAbortHandle::new_pair();
        let observer = handle.clone();
        let abort_handle = TaskAbortHandle::Local {
            handle,
            finished: Arc::new(AtomicBool::new(false)),
        };
        scope.register(id, abort_handle.clone());
        let (_, receiver) = oneshot::channel();
        (
            Task::from_local_receiver(id, receiver, abort_handle, Some(scope.clone())),
            observer,
        )
    }

    #[test]
    fn dropping_awaitable_tasks_aborts_and_unregisters() {
        let scope = TaskScope::new();
        let (task, registration) = local_task(&scope);
        assert_eq!(scope.task_count(), 1);
        drop(task);
        assert!(registration.is_aborted());
        assert_eq!(scope.task_count(), 0);
    }

    #[test]
    fn detached_tasks_remain_until_the_scope_closes() {
        let scope = TaskScope::new();
        let (task, registration) = local_task(&scope);
        task.detach();
        assert!(!registration.is_aborted());
        assert_eq!(scope.task_count(), 1);
        scope.close();
        assert!(registration.is_aborted());
        assert!(scope.is_closed());
        assert_eq!(scope.task_count(), 0);
        let (task, registration) = local_task(&scope);
        assert!(registration.is_aborted());
        assert_eq!(scope.task_count(), 0);
        drop(task);
    }

    #[test]
    fn awaitable_tasks_deliver_values_and_notify_completion() {
        let (sender, receiver) = oneshot::channel();
        let (handle, _) = LocalAbortHandle::new_pair();
        let mut task = Task::from_local_receiver(
            TaskId::next_untracked(),
            receiver,
            TaskAbortHandle::Local {
                handle,
                finished: Arc::new(AtomicBool::new(true)),
            },
            None,
        );
        let observed = Arc::new(AtomicBool::new(false));
        let completed = observed.clone();
        task.on_completion(move |value| {
            assert_eq!(value.downcast_ref::<usize>(), Some(&42));
            completed.store(true, Ordering::Release);
        });
        sender.send(42usize).unwrap();
        assert_eq!(futures::executor::block_on(task), 42);
        assert!(observed.load(Ordering::Acquire));
    }
}
