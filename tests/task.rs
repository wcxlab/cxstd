use std::{
    cell::Cell,
    future::{pending, poll_fn},
    rc::Rc,
    task::Poll,
    time::{Duration, Instant},
};

use cxstd::{
    Result,
    ecs::{TaskExecutor, TaskScope},
};

fn executor() -> Result<(tokio::runtime::Runtime, TaskExecutor)> {
    let host = tokio::runtime::Runtime::new()?;
    let executor = {
        let _entered = host.enter();
        TaskExecutor::new()?
    };
    Ok((host, executor))
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready() {
        assert!(Instant::now() < deadline, "async operation timed out");
        std::thread::yield_now();
    }
}

#[test]
fn construction_requires_a_host_runtime() {
    assert!(
        std::thread::spawn(|| TaskExecutor::new().is_err())
            .join()
            .unwrap()
    );
}

#[test]
fn background_completion_waits_for_owner_update_outside_tokio_scope() -> Result<()> {
    let (_host, executor) = executor()?;
    let output = Rc::new(Cell::new(0));
    let completed = output.clone();
    let owner = std::thread::current().id();
    let task = executor.spawn(async { 42 }, move |value| {
        assert_eq!(std::thread::current().id(), owner);
        completed.set(value);
    })?;
    wait_until(|| task.is_finished());
    assert_eq!(output.get(), 0);
    wait_until(|| {
        executor.update();
        output.get() == 42
    });
    assert!(task.is_finished());
    assert_eq!(executor.pending_count(), 0);
    executor.close();
    Ok(())
}

#[test]
fn non_send_foreground_tasks_are_polled_only_when_runnable() -> Result<()> {
    let (_host, executor) = executor()?;
    let polls = Rc::new(Cell::new(0));
    let observed = polls.clone();
    let task = executor.spawn_local(poll_fn(move |cx| {
        observed.set(observed.get() + 1);
        if observed.get() == 1 {
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(Rc::new(7))
        }
    }))?;
    assert_eq!(polls.get(), 0);
    executor.update();
    assert_eq!(polls.get(), 2);
    assert_eq!(*futures::executor::block_on(task), 7);
    executor.update();
    assert_eq!(polls.get(), 2);
    executor.close();
    Ok(())
}

#[test]
fn foreground_timers_use_the_captured_host_runtime() -> Result<()> {
    let (_host, executor) = executor()?;
    let task = executor.spawn_local(async {
        tokio::time::sleep(Duration::from_millis(1)).await;
        42
    })?;
    wait_until(|| {
        executor.update();
        task.is_finished()
    });
    assert_eq!(futures::executor::block_on(task), 42);
    executor.close();
    Ok(())
}

#[test]
fn deferred_callbacks_consume_one_batch_per_explicit_update() -> Result<()> {
    let (_host, executor) = executor()?;
    let calls = Rc::new(Cell::new(0));
    let next_runtime = executor.clone();
    let first = calls.clone();
    executor.defer(move || {
        first.set(first.get() + 1);
        next_runtime.defer(move || first.set(first.get() + 1));
    });
    assert_eq!(calls.get(), 0);
    assert_eq!(executor.update(), 1);
    assert_eq!(calls.get(), 1);
    assert_eq!(executor.update(), 1);
    assert_eq!(calls.get(), 2);
    executor.close();
    Ok(())
}

#[test]
fn cross_thread_callbacks_keep_queued_payloads_alive_until_owner_delivery() -> Result<()> {
    let (_host, executor) = executor()?;
    let value = Rc::new(Cell::new(0));
    let delivered = value.clone();
    let callback = executor.bind(move |input: usize| delivered.set(input));
    std::thread::spawn(move || callback.send(9).unwrap())
        .join()
        .unwrap();
    assert_eq!(value.get(), 0);
    executor.update();
    assert_eq!(value.get(), 9);
    let late = executor.bind(|_: ()| panic!("closed callback must not run"));
    executor.close();
    assert!(late.send(()).is_err());
    Ok(())
}

#[test]
fn scopes_cancel_foreground_tasks_and_reject_late_spawns() -> Result<()> {
    let (_host, executor) = executor()?;
    let scope = TaskScope::new();
    let task = executor.spawn_local_scoped(pending::<()>(), Some(scope.clone()))?;
    assert_eq!(scope.task_count(), 1);
    scope.close();
    executor.update();
    assert!(task.is_finished());
    assert_eq!(scope.task_count(), 0);
    assert!(executor.spawn_local_scoped(async {}, Some(scope)).is_err());
    executor.close();
    assert!(executor.spawn(async {}, |_| {}).is_err());
    assert!(executor.spawn_local(async {}).is_err());
    Ok(())
}

#[test]
fn closing_during_update_stops_remaining_deferred_callbacks() -> Result<()> {
    let (_host, executor) = executor()?;
    let closer = executor.clone();
    executor.defer(move || closer.close());
    executor.defer(|| panic!("close cancels remaining callbacks"));
    executor.update();
    Ok(())
}

#[test]
fn nested_updates_leave_newly_deferred_work_for_the_next_outer_update() -> Result<()> {
    let (_host, executor) = executor()?;
    let nested = executor.clone();
    let calls = Rc::new(Cell::new(0));
    let value = calls.clone();
    executor.defer(move || {
        nested.defer(move || value.set(1));
        assert_eq!(nested.update(), 0);
    });
    executor.update();
    assert_eq!(calls.get(), 0);
    executor.update();
    assert_eq!(calls.get(), 1);
    executor.close();
    Ok(())
}

#[test]
fn a_foreground_task_can_close_its_executor_without_being_reinserted() -> Result<()> {
    let (_host, executor) = executor()?;
    let closer = executor.clone();
    let task = executor.spawn_local(async move {
        closer.close();
        pending::<()>().await;
    })?;
    executor.update();
    assert!(task.is_finished());
    assert_eq!(executor.update(), 0);
    Ok(())
}

#[test]
fn cancellation_scope_can_track_tasks_from_multiple_executors() -> Result<()> {
    let (host, first) = executor()?;
    let second = {
        let _entered = host.enter();
        TaskExecutor::new()?
    };
    let scope = TaskScope::new();
    let first_task = first.spawn_local_scoped(pending::<()>(), Some(scope.clone()))?;
    let second_task = second.spawn_local_scoped(pending::<()>(), Some(scope.clone()))?;
    assert_ne!(first_task.id(), second_task.id());
    assert_eq!(scope.task_count(), 2);
    scope.close();
    first.update();
    second.update();
    assert!(first_task.is_finished());
    assert!(second_task.is_finished());
    assert_eq!(scope.task_count(), 0);
    first.close();
    second.close();
    Ok(())
}

#[test]
fn detached_background_tasks_release_scopes_on_success_and_panic() -> Result<()> {
    let (_host, executor) = executor()?;
    let scope = TaskScope::new();
    let background = executor.scoped_background_executor(scope.clone());
    for _ in 0..64 {
        background.spawn(async {}).detach();
    }
    background
        .spawn(async { panic!("expected worker panic") })
        .detach();
    wait_until(|| scope.task_count() == 0);
    executor.close();
    Ok(())
}
