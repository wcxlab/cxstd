//! Owner-thread task scheduling through `TaskExecutor::new()` and `update()`.
//! Futures, completion callbacks and cross-thread callback delivery are independent
//! of views, elements and rendering systems.

use std::task::{Context as TaskContext, Poll, Wake, Waker};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, Sender, TryRecvError, channel},
    },
    thread::{self, ThreadId},
};

use anyhow::Context as _;
use futures::channel::oneshot;
use futures::future::{AbortHandle as LocalAbortHandle, Abortable};
use tokio::task::AbortHandle;

pub use super::handle::{AsyncClosed, Task, TaskId};
pub(crate) use super::handle::{TaskAbortHandle, TaskScope};

type TaskOutput = Box<dyn Any + Send>;
type CompletionCallback = Box<dyn FnOnce(TaskOutput)>;
struct CallbackInvocation {
    id: u64,
    value: TaskOutput,
    _lifetime: Arc<()>,
}

struct CallbackEntry {
    callback: Rc<dyn Fn(TaskOutput)>,
    lifetime: std::sync::Weak<()>,
}

struct Completion {
    task_id: TaskId,
    output: Option<TaskOutput>,
}

struct PendingTask {
    abort_handle: AbortHandle,
    callback: CompletionCallback,
    completion_callbacks: Arc<Mutex<Vec<Box<dyn FnOnce(&dyn Any) + Send>>>>,
    scope: Option<TaskScope>,
}

struct Shared {
    tokio: tokio::runtime::Handle,
    completion_tx: Sender<Completion>,
    callback_tx: Sender<CallbackInvocation>,
    closed: Arc<AtomicBool>,
    next_callback_id: AtomicU64,
    local_ready: Mutex<std::collections::VecDeque<TaskId>>,
}

#[derive(Clone)]
pub struct TaskExecutor {
    shared: Arc<Shared>,
    main: Rc<MainThreadState>,
}

struct MainThreadState {
    updating: Cell<bool>,
    thread_id: ThreadId,
    completion_rx: Receiver<Completion>,
    callback_rx: Receiver<CallbackInvocation>,
    pending: RefCell<HashMap<TaskId, PendingTask>>,
    deferred: RefCell<Vec<Box<dyn FnOnce()>>>,
    callbacks: RefCell<HashMap<u64, CallbackEntry>>,
    local_tasks: RefCell<HashMap<TaskId, LocalTask>>,
}

struct LocalTask {
    future: Pin<Box<dyn Future<Output = ()> + 'static>>,
    waker: Arc<LocalTaskWaker>,
    finished: Arc<AtomicBool>,
    scope: Option<TaskScope>,
}

struct LocalTaskWaker {
    task_id: TaskId,
    shared: Arc<Shared>,
    queued: AtomicBool,
}

impl Wake for LocalTaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if !self.queued.swap(true, Ordering::AcqRel) {
            self.shared
                .local_ready
                .lock()
                .unwrap()
                .push_back(self.task_id);
        }
    }
}

/// A thread-safe sender for a callback bound to the executor's owning thread.
pub struct AsyncCallback<T> {
    id: u64,
    shared: Arc<Shared>,
    lifetime: Arc<()>,
    marker: std::marker::PhantomData<fn(T)>,
}

impl TaskExecutor {
    pub fn new() -> anyhow::Result<Self> {
        let tokio =
            tokio::runtime::Handle::try_current().context("an active Tokio runtime is required")?;
        let (completion_tx, completion_rx) = channel();
        let (callback_tx, callback_rx) = channel();
        Ok(Self {
            shared: Arc::new(Shared {
                tokio,
                completion_tx,
                callback_tx,
                closed: Arc::new(AtomicBool::new(false)),
                next_callback_id: AtomicU64::new(0),
                local_ready: Mutex::new(std::collections::VecDeque::new()),
            }),
            main: Rc::new(MainThreadState {
                updating: Cell::new(false),
                thread_id: thread::current().id(),
                completion_rx,
                callback_rx,
                pending: RefCell::new(HashMap::new()),
                deferred: RefCell::new(Vec::new()),
                callbacks: RefCell::new(HashMap::new()),
                local_tasks: RefCell::new(HashMap::new()),
            }),
        })
    }

    /// Use the captured host Tokio handle, including outside a Tokio enter scope.
    pub fn background_executor(&self) -> super::BackgroundExecutor {
        super::BackgroundExecutor::new(self)
    }

    pub fn scoped_background_executor(&self, scope: TaskScope) -> super::BackgroundExecutor {
        super::BackgroundExecutor::new_for_scope(self, scope)
    }

    pub fn foreground_executor(&self, scope: Option<TaskScope>) -> super::ForegroundExecutor {
        super::ForegroundExecutor::new(self, scope)
    }

    /// Run a Send future on Tokio, then deliver its result on the owning thread.
    pub fn spawn<F, O>(
        &self,
        future: F,
        callback: impl FnOnce(O) + 'static,
    ) -> Result<Task<O>, AsyncClosed>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        self.spawn_boxed(
            future,
            Box::new(move |output| {
                let output = output
                    .downcast::<O>()
                    .expect("task output must match its registered type");
                callback(*output);
            }),
            None,
        )
    }

    /// Bind a local callback and obtain a sender that can be used from any thread.
    pub fn bind<T: Send + 'static>(&self, callback: impl Fn(T) + 'static) -> AsyncCallback<T> {
        self.assert_main_thread();
        let id = self.shared.next_callback_id.fetch_add(1, Ordering::Relaxed);
        let lifetime = Arc::new(());
        self.main.callbacks.borrow_mut().insert(
            id,
            CallbackEntry {
                callback: Rc::new(move |value| {
                    let value = value
                        .downcast::<T>()
                        .expect("callback payload must match its registered type");
                    callback(*value);
                }),
                lifetime: Arc::downgrade(&lifetime),
            },
        );
        AsyncCallback {
            id,
            shared: Arc::clone(&self.shared),
            lifetime,
            marker: std::marker::PhantomData,
        }
    }

    /// Callback-driven background tasks awaiting owner-thread delivery.
    pub fn pending_count(&self) -> usize {
        self.main.pending.borrow().len()
    }

    /// Spawn an awaitable task on the captured host Tokio runtime.
    pub fn spawn_awaitable<F, O>(&self, future: F) -> Result<Task<O>, AsyncClosed>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        self.assert_main_thread();
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(AsyncClosed);
        }

        let task_id = TaskId::next_untracked();
        let task = self.shared.tokio.spawn(future);
        Ok(Task::from_join_handle(task_id, task, None))
    }

    /// Run a non-Send future on the executor's owning thread.
    pub fn spawn_local<F, O>(&self, future: F) -> Result<Task<O>, AsyncClosed>
    where
        F: Future<Output = O> + 'static,
        O: 'static,
    {
        self.spawn_local_scoped(future, None)
    }

    /// Queue a non-`Send` future for polling by `update` when it becomes runnable.
    pub fn spawn_local_scoped<F, O>(
        &self,
        future: F,
        scope: Option<TaskScope>,
    ) -> Result<Task<O>, AsyncClosed>
    where
        F: Future<Output = O> + 'static,
        O: 'static,
    {
        self.assert_main_thread();
        if self.shared.closed.load(Ordering::Acquire)
            || scope.as_ref().is_some_and(TaskScope::is_closed)
        {
            return Err(AsyncClosed);
        }

        let task_id = TaskId::next_untracked();
        let (abort_handle, abort_registration) = LocalAbortHandle::new_pair();
        let finished = Arc::new(AtomicBool::new(false));
        let finished_for_task = Arc::clone(&finished);
        let (sender, receiver) = oneshot::channel();
        let task_future = async move {
            if let Ok(output) = Abortable::new(future, abort_registration).await {
                let _ = sender.send(output);
            }
            finished_for_task.store(true, Ordering::Release);
        };
        let waker_state = Arc::new(LocalTaskWaker {
            task_id,
            shared: Arc::clone(&self.shared),
            queued: AtomicBool::new(false),
        });
        let abort_handle = TaskAbortHandle::Local {
            handle: abort_handle,
            finished: Arc::clone(&finished),
        };
        if let Some(scope) = &scope {
            scope.register(task_id, abort_handle.clone());
        }
        self.main.local_tasks.borrow_mut().insert(
            task_id,
            LocalTask {
                future: Box::pin(task_future),
                waker: waker_state.clone(),
                finished,
                scope: scope.clone(),
            },
        );
        waker_state.wake_by_ref();

        Ok(Task::from_local_receiver(
            task_id,
            receiver,
            abort_handle,
            scope,
        ))
    }

    /// Advance ready work without waiting. The host calls this periodically on
    /// the owning thread. Returns callbacks/completions processed in this pass.
    /// Pending foreground futures are polled only when their Wakers queue them.
    pub fn update(&self) -> usize {
        self.assert_main_thread();
        if self.shared.closed.load(Ordering::Acquire) {
            return 0;
        }
        // A callback may reach the executor through its context. Nested updates
        // leave newly queued work for the next outer owner-thread update.
        if self.main.updating.replace(true) {
            return 0;
        }
        let _update = UpdateScope(&self.main.updating);
        let _entered = self.shared.tokio.enter();
        let mut completions = Vec::new();
        loop {
            match self.main.completion_rx.try_recv() {
                Ok(completion) => completions.push(completion),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }

        let mut completed = 0;
        for completion in completions {
            if self.shared.closed.load(Ordering::Acquire) {
                break;
            }
            let pending = self.main.pending.borrow_mut().remove(&completion.task_id);
            if let Some(scope) = pending.as_ref().and_then(|pending| pending.scope.as_ref()) {
                scope.unregister(completion.task_id);
            }
            if let (Some(pending), Some(output)) = (pending, completion.output) {
                let callbacks = std::mem::take(&mut *pending.completion_callbacks.lock().unwrap());
                callbacks
                    .into_iter()
                    .for_each(|callback| callback(&*output));
                (pending.callback)(output);
                completed += 1;
            }
        }

        let mut invocations = Vec::new();
        while let Ok(invocation) = self.main.callback_rx.try_recv() {
            invocations.push(invocation);
        }
        let mut invoked = 0;
        for invocation in invocations {
            if self.shared.closed.load(Ordering::Acquire) {
                break;
            }
            let callback = self
                .main
                .callbacks
                .borrow()
                .get(&invocation.id)
                .map(|entry| entry.callback.clone());
            if let Some(callback) = callback {
                callback(invocation.value);
                invoked += 1;
            }
        }
        self.main
            .callbacks
            .borrow_mut()
            .retain(|_, entry| entry.lifetime.strong_count() > 0);

        let deferred = std::mem::take(&mut *self.main.deferred.borrow_mut());
        let mut deferred_count = 0;
        for callback in deferred {
            if self.shared.closed.load(Ordering::Acquire) {
                break;
            }
            callback();
            deferred_count += 1;
        }

        let local_count = self.poll_local_tasks();
        completed + deferred_count + invoked + local_count
    }

    pub fn defer(&self, callback: impl FnOnce() + 'static) {
        self.assert_main_thread();
        if self.shared.closed.load(Ordering::Acquire) {
            return;
        }
        self.main.deferred.borrow_mut().push(Box::new(callback));
    }

    pub fn closed_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shared.closed)
    }

    pub fn close(&self) {
        self.assert_main_thread();
        self.shared.closed.store(true, Ordering::Release);
        let pending = std::mem::take(&mut *self.main.pending.borrow_mut());
        for (task_id, pending) in pending {
            pending.abort_handle.abort();
            if let Some(scope) = &pending.scope {
                scope.unregister(task_id);
            }
        }
        while self.main.completion_rx.try_recv().is_ok() {}
        while self.main.callback_rx.try_recv().is_ok() {}
        drop(std::mem::take(&mut *self.main.deferred.borrow_mut()));
        drop(std::mem::take(&mut *self.main.callbacks.borrow_mut()));
        let local_tasks = std::mem::take(&mut *self.main.local_tasks.borrow_mut());
        for (task_id, task) in local_tasks {
            task.finished.store(true, Ordering::Release);
            if let Some(scope) = task.scope {
                scope.unregister(task_id);
            }
        }
        self.shared.local_ready.lock().unwrap().clear();
    }
}

impl TaskExecutor {
    pub(super) fn tokio_handle(&self) -> tokio::runtime::Handle {
        self.shared.tokio.clone()
    }

    /// Runs a future on the tokio runtime and delivers its output to a callback
    /// on the main thread.
    fn spawn_boxed<F, O>(
        &self,
        future: F,
        callback: CompletionCallback,
        scope: Option<TaskScope>,
    ) -> Result<Task<O>, AsyncClosed>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        self.assert_main_thread();
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(AsyncClosed);
        }
        if scope.as_ref().is_some_and(TaskScope::is_closed) {
            return Err(AsyncClosed);
        }

        let task_id = TaskId::next_untracked();
        let worker = self.shared.tokio.spawn(future);
        let abort_handle = worker.abort_handle();
        let completion_callbacks = Arc::new(Mutex::new(Vec::new()));
        self.main.pending.borrow_mut().insert(
            task_id,
            PendingTask {
                abort_handle: abort_handle.clone(),
                callback,
                completion_callbacks: completion_callbacks.clone(),
                scope: scope.clone(),
            },
        );
        if let Some(scope) = &scope {
            scope.register(task_id, TaskAbortHandle::Tokio(abort_handle.clone()));
        }

        let shared = Arc::clone(&self.shared);
        self.shared.tokio.spawn(async move {
            let output = worker
                .await
                .ok()
                .map(|output| Box::new(output) as TaskOutput);
            if !shared.closed.load(Ordering::Acquire) {
                let _ = shared.completion_tx.send(Completion { task_id, output });
            }
        });

        Ok(Task::from_parts(
            task_id,
            TaskAbortHandle::Tokio(abort_handle),
            scope,
            completion_callbacks,
        ))
    }

    fn poll_local_tasks(&self) -> usize {
        const POLL_BUDGET: usize = 256;

        let mut completed = 0;
        for _ in 0..POLL_BUDGET {
            let task_id = self.shared.local_ready.lock().unwrap().pop_front();
            let Some(task_id) = task_id else {
                break;
            };
            let Some(mut task) = self.main.local_tasks.borrow_mut().remove(&task_id) else {
                continue;
            };
            task.waker.queued.store(false, Ordering::Release);
            let waker = Waker::from(Arc::clone(&task.waker));
            let mut context = TaskContext::from_waker(&waker);
            match task.future.as_mut().poll(&mut context) {
                Poll::Ready(()) => {
                    task.finished.store(true, Ordering::Release);
                    if let Some(scope) = &task.scope {
                        scope.unregister(task_id);
                    }
                    completed += 1;
                }
                Poll::Pending => {
                    if self.shared.closed.load(Ordering::Acquire) {
                        task.finished.store(true, Ordering::Release);
                        if let Some(scope) = &task.scope {
                            scope.unregister(task_id);
                        }
                    } else {
                        self.main.local_tasks.borrow_mut().insert(task_id, task);
                    }
                }
            }
        }

        completed
    }

    fn assert_main_thread(&self) {
        assert_eq!(
            thread::current().id(),
            self.main.thread_id,
            "task executor must only be accessed from its owning thread"
        );
    }
}

impl<T: Send + 'static> AsyncCallback<T> {
    pub fn send(&self, value: T) -> Result<(), AsyncClosed> {
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(AsyncClosed);
        }
        self.shared
            .callback_tx
            .send(CallbackInvocation {
                id: self.id,
                value: Box::new(value),
                _lifetime: self.lifetime.clone(),
            })
            .map_err(|_| AsyncClosed)?;
        Ok(())
    }
}

impl<T> Clone for AsyncCallback<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            shared: Arc::clone(&self.shared),
            lifetime: self.lifetime.clone(),
            marker: std::marker::PhantomData,
        }
    }
}

struct UpdateScope<'a>(&'a Cell<bool>);

impl Drop for UpdateScope<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
