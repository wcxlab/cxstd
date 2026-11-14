use std::{
    cell::Cell,
    collections::HashSet,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use cxstd::ecs::EventDispatcher;

#[test]
fn queued_events_route_by_key_and_type_without_clone_or_send_bounds() {
    struct LocalEvent(Rc<String>);
    let events = EventDispatcher::<String, Vec<String>>::default();
    let text = events.subscribe("input".into(), |event: &LocalEvent, context| {
        context.push((*event.0).clone());
    });
    let unit = events.subscribe("input".into(), |_: &(), context| {
        context.push("unit".into())
    });
    let changed = events.observe("input".into(), |context| context.push("changed".into()));
    events.emit("other".into(), LocalEvent(Rc::new("ignored".into())));
    events.emit("input".into(), LocalEvent(Rc::new("text".into())));
    events.emit("input".into(), 42_u32);
    events.notify("input".into());
    events.emit("input".into(), ());
    let mut context = Vec::new();
    assert_eq!(events.pending_count(), 5);
    assert_eq!(events.dispatch(&mut context), 5);
    assert_eq!(context, ["text", "changed", "unit"]);
    assert_eq!(events.pending_count(), 0);
    drop((text, unit, changed));
}

#[test]
fn clones_share_queue_and_nested_dispatch_preserves_the_next_batch() {
    let events = EventDispatcher::<u64, Vec<u32>>::default();
    let nested = events.clone();
    let first = events.subscribe(1, move |value: &u32, context| {
        context.push(*value);
        nested.notify(2);
        assert_eq!(nested.dispatch(context), 0);
    });
    let second = events.observe(2, |context| context.push(99));
    events.clone().emit(1, 7_u32);
    let mut context = Vec::new();
    assert_eq!(events.dispatch(&mut context), 1);
    assert_eq!(context, [7]);
    assert_eq!(events.pending_count(), 1);
    assert_eq!(events.dispatch(&mut context), 1);
    assert_eq!(context, [7, 99]);
    drop((first, second));
}

#[test]
fn removing_a_target_cancels_inflight_events_before_reusing_its_key() {
    let events = EventDispatcher::<u64, Vec<u32>>::default();
    let nested = events.clone();
    let first = events.subscribe(1, move |_: &(), context| {
        context.push(1);
        nested.remove_key(&2);
        nested
            .subscribe(2, |value: &u32, context| context.push(*value))
            .detach();
        nested.emit(2, 3_u32);
    });
    let old = events.subscribe(2, |_: &u32, _| panic!("removed observer must not run"));
    events.emit(1, ());
    events.emit(2, 2_u32);
    let mut context = Vec::new();
    assert_eq!(events.dispatch(&mut context), 1);
    assert_eq!(context, [1]);
    assert_eq!(events.dispatch(&mut context), 1);
    assert_eq!(context, [1, 3]);
    events.remove_key(&2);
    drop((first, old));
}

#[test]
fn target_liveness_is_checked_again_between_callbacks() {
    struct Context {
        live: HashSet<u64>,
        calls: Vec<u32>,
    }
    let events = EventDispatcher::<u64, Context>::default();
    let first = events.observe(1, |context| {
        context.calls.push(1);
        context.live.remove(&2);
    });
    let second = events.observe(2, |_| panic!("dead target must be skipped"));
    events.notify(1);
    events.notify(2);
    let mut context = Context {
        live: [1, 2].into(),
        calls: Vec::new(),
    };
    assert_eq!(
        events.dispatch_if(&mut context, |key, cx| cx.live.contains(key)),
        1
    );
    assert_eq!(context.calls, [1]);
    drop((first, second));
}

#[test]
fn removed_payloads_can_enqueue_work_without_borrowing_the_queue_recursively() {
    struct PostOnDrop(EventDispatcher<u64, Vec<u32>>);
    impl Drop for PostOnDrop {
        fn drop(&mut self) {
            self.0.emit(2, 9_u32);
        }
    }
    let events = EventDispatcher::<u64, Vec<u32>>::default();
    let second = events.subscribe(2, |value: &u32, context| context.push(*value));
    events.emit(1, PostOnDrop(events.clone()));
    events.remove_key(&1);
    assert_eq!(events.pending_count(), 1);
    let mut context = Vec::new();
    events.dispatch(&mut context);
    assert_eq!(context, [9]);
    drop(second);
}

#[test]
fn a_panicking_callback_releases_the_dispatch_guard() {
    let events = EventDispatcher::<u64, Vec<u32>>::default();
    let panics = Rc::new(Cell::new(true));
    let once = panics.clone();
    let subscription = events.observe(1, move |context| {
        if once.replace(false) {
            panic!("expected callback panic");
        }
        context.push(1);
    });
    let mut context = Vec::new();
    events.notify(1);
    assert!(catch_unwind(AssertUnwindSafe(|| events.dispatch(&mut context))).is_err());
    events.notify(1);
    assert_eq!(events.dispatch(&mut context), 1);
    assert_eq!(context, [1]);
    drop(subscription);
}

#[test]
fn callbacks_borrow_two_independent_execution_values() {
    let events = EventDispatcher::<u64, Vec<u32>, u32>::default();
    let subscription = events.subscribe_with(1, |value: &u32, output, total| {
        *total += value;
        output.push(*total);
    });
    events.emit(1, 7_u32);
    events.emit(1, 5_u32);
    let mut output = Vec::new();
    let mut total = 0;
    assert_eq!(events.dispatch_with(&mut output, &mut total), 2);
    assert_eq!(output, [7, 12]);
    assert_eq!(total, 12);
    drop(subscription);
}
