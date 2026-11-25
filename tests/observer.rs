use std::{
    any::TypeId,
    cell::{Cell, RefCell},
    rc::Rc,
};

use cxstd::ecs::{ObserverRegistry, Observers, Subscription};

#[test]
fn registry_routes_arbitrary_handlers_and_invalidates_removed_ids() {
    struct SystemId(u32);
    let mut registry = ObserverRegistry::default();
    let first = registry.insert("changed", SystemId(1));
    let second = registry.insert("changed", SystemId(2));
    let other = registry.insert("removed", SystemId(3));

    assert_eq!(
        registry.matching(&"changed").collect::<Vec<_>>(),
        [first, second]
    );
    assert_eq!(registry.get(other).unwrap().0, 3);
    assert_eq!(registry.remove(first).unwrap().0, 1);
    assert!(registry.get(first).is_none());
    assert!(registry.remove(first).is_none());
    assert_eq!(registry.matching(&"changed").collect::<Vec<_>>(), [second]);
    registry.remove(second);
    assert_eq!(registry.matching(&"changed").count(), 0);
    let replacement = registry.insert("changed", SystemId(4));
    assert_ne!(first, replacement);
    assert_ne!(second, replacement);

    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ObserverRegistry<String, SystemId>>();
}

#[test]
fn typed_callbacks_filter_keys_and_types_and_share_clones() {
    #[derive(Clone, Eq, Hash, PartialEq)]
    struct ObjectKey(String);
    struct Context(Vec<u32>);

    let observers = Observers::<ObjectKey, Context>::default();
    let key = ObjectKey("first".into());
    let mut calls = 0;
    let subscription = observers.subscribe(key.clone(), move |value: &u32, context| {
        calls += 1;
        context.0.push(*value + calls);
    });
    let mut context = Context(Vec::new());
    observers.trigger(ObjectKey("second".into()), &1_u32, &mut context);
    observers.trigger(key.clone(), &2_u64, &mut context);
    observers.clone().trigger(key.clone(), &3_u32, &mut context);
    drop(subscription);
    observers.trigger(key, &4_u32, &mut context);
    assert_eq!(context.0, [4]);
}

#[test]
fn global_observers_run_before_keyed_observers_in_group_order() {
    let observers = Observers::<u64, Vec<u32>>::default();
    let keyed = observers.subscribe(1, |_: &u32, context| context.push(3));
    let global = observers.subscribe_global(|_: &u32, context| context.push(1));
    let another_global = observers.subscribe_global(|_: &u32, context| context.push(2));
    let mut context = Vec::new();

    observers.trigger(1, &0_u32, &mut context);
    assert_eq!(context, [1, 2, 3]);
    context.clear();
    observers.trigger(2, &0_u32, &mut context);
    assert_eq!(context, [1, 2]);
    context.clear();
    observers.trigger_global(&0_u32, &mut context);
    assert_eq!(context, [1, 2]);
    drop((keyed, global, another_global));
}

#[test]
fn dispatch_defers_new_registrations_and_skips_cancelled_observers() {
    let observers = Observers::<u64, Vec<u32>>::default();
    let second = Rc::new(RefCell::new(None));
    let added = Rc::new(RefCell::new(None));
    let registration = added.clone();
    let removed = second.clone();
    let bus = observers.clone();
    let first = observers.subscribe(1, move |_: &(), context| {
        context.push(1);
        removed.borrow_mut().take();
        if registration.borrow().is_none() {
            *registration.borrow_mut() = Some(bus.subscribe(1, |_: &(), context| context.push(3)));
        }
    });
    *second.borrow_mut() = Some(observers.subscribe(1, |_: &(), context| context.push(2)));

    let mut context = Vec::new();
    observers.trigger(1, &(), &mut context);
    assert_eq!(context, [1]);
    observers.trigger(1, &(), &mut context);
    assert_eq!(context, [1, 1, 3]);
    drop(first);
}

#[test]
fn callback_can_cancel_itself_and_trigger_another_type() {
    let observers = Observers::<u64, Vec<u32>>::default();
    let nested = observers.subscribe(1, |value: &u32, context| context.push(*value));
    let slot = Rc::new(RefCell::new(None));
    let captured = slot.clone();
    let bus = observers.clone();
    *slot.borrow_mut() = Some(observers.subscribe(1, move |_: &(), context| {
        captured.borrow_mut().take();
        bus.trigger(1, &42_u32, context);
    }));

    let mut context = Vec::new();
    observers.trigger(1, &(), &mut context);
    observers.trigger(1, &(), &mut context);
    assert_eq!(context, [42]);
    assert!(slot.borrow().is_none());
    drop(nested);
}

#[test]
fn change_notifications_are_distinct_from_typed_unit_events() {
    let observers = Observers::<u64, Vec<u32>>::default();
    let change = observers.observe(1, |context| context.push(1));
    let event = observers.subscribe(1, |_: &(), context| context.push(2));
    let mut context = Vec::new();

    observers.notify(2, &mut context);
    observers.notify(1, &mut context);
    assert_eq!(context, [1]);
    observers.trigger(1, &(), &mut context);
    assert_eq!(context, [1, 2]);
    observers.notify(1, &mut context);
    assert_eq!(context, [1, 2, 1]);
    drop((change, event));
}

#[test]
fn resource_notifications_use_type_ids_and_caller_context() {
    struct Theme;
    struct Other;
    let observers = Observers::<TypeId, Vec<u32>>::default();
    let handle = observers.observe(TypeId::of::<Theme>(), |context| context.push(1));
    let mut context = Vec::new();
    observers.notify(TypeId::of::<Other>(), &mut context);
    observers.notify(TypeId::of::<Theme>(), &mut context);
    drop(handle);
    observers.notify(TypeId::of::<Theme>(), &mut context);
    assert_eq!(context, [1]);
}

#[test]
fn host_defers_dispatch_until_after_state_borrows_are_released() {
    struct Context {
        value: Rc<RefCell<String>>,
        pending: Vec<u64>,
    }
    let observers = Observers::<u64, Context>::default();
    let observed = Rc::new(RefCell::new(String::new()));
    let output = observed.clone();
    let bus = observers.clone();
    let handle = observers.observe(1, move |context| {
        *output.borrow_mut() = context.value.borrow().clone();
        let temporary = bus.observe(2, |_| {});
        drop(temporary);
        context.pending.push(2);
    });
    let mut context = Context {
        value: Rc::new(RefCell::new(String::new())),
        pending: Vec::new(),
    };
    {
        let mut value = context.value.borrow_mut();
        *value = "updated".into();
        context.pending.push(1);
        assert!(observed.borrow().is_empty());
    }
    for key in std::mem::take(&mut context.pending) {
        observers.notify(key, &mut context);
    }
    assert_eq!(*observed.borrow(), "updated");
    assert_eq!(context.pending, [2]);
    drop(handle);
}

#[test]
fn local_callbacks_accept_non_send_non_clone_payloads() {
    struct Payload(Rc<Cell<u32>>);
    let observers = Observers::<u64>::default();
    let handle = observers.subscribe(1, |event: &Payload, _: &mut ()| event.0.set(42));
    let payload = Payload(Rc::new(Cell::new(0)));
    observers.trigger(1, &payload, &mut ());
    assert_eq!(payload.0.get(), 42);
    drop(handle);
}

#[test]
fn subscriptions_do_not_keep_callbacks_alive_after_registry_drops() {
    let observers = Observers::<u64>::default();
    let captured = Rc::new(());
    let kept = captured.clone();
    let handle = observers.observe(1, move |_| {
        let _ = &kept;
    });
    drop(observers);
    assert_eq!(Rc::strong_count(&captured), 1);
    drop(handle);
}

#[test]
fn dropping_callback_captures_can_cancel_other_subscriptions() {
    let observers = Observers::<u64>::default();
    let second = observers.observe(2, |_| {});
    let first = observers.observe(1, move |_| {
        let _ = &second;
    });
    drop(first);
    observers.notify(2, &mut ());
}

#[test]
fn detached_callbacks_remain_until_registry_is_dropped() {
    let observers = Observers::<u64>::default();
    let calls = Rc::new(Cell::new(0));
    let output = calls.clone();
    observers
        .observe(1, move |_| output.set(output.get() + 1))
        .detach();
    observers.notify(1, &mut ());
    assert_eq!(calls.get(), 1);
    assert_eq!(Rc::strong_count(&calls), 2);
    drop(observers);
    assert_eq!(Rc::strong_count(&calls), 1);
}

#[test]
fn subscription_cancels_once_unless_detached() {
    let calls = Rc::new(Cell::new(0));
    let output = calls.clone();
    drop(Subscription::new(move || output.set(output.get() + 1)));
    let output = calls.clone();
    Subscription::new(move || output.set(output.get() + 1)).detach();
    assert_eq!(calls.get(), 1);
}

#[test]
fn object_cleanup_removes_all_local_routes_and_preserves_global_observers() {
    let observers = Observers::<u64, Vec<u32>>::default();
    let captured = Rc::new(());
    let kept = captured.clone();
    observers
        .observe(1, move |context| {
            let _ = &kept;
            context.push(1);
        })
        .detach();
    observers
        .subscribe(1, |_: &u32, context| context.push(2))
        .detach();
    observers
        .subscribe(1, |_: &(), context| context.push(3))
        .detach();
    let global = observers.subscribe_global(|_: &u32, context| context.push(4));
    let other = observers.observe(2, |context| context.push(5));

    observers.remove_key(&1);
    observers.remove_key(&1);
    assert_eq!(Rc::strong_count(&captured), 1);
    let mut context = Vec::new();
    observers.notify(1, &mut context);
    observers.trigger(1, &(), &mut context);
    observers.trigger(1, &0_u32, &mut context);
    observers.notify(2, &mut context);
    assert_eq!(context, [4, 5]);
    drop((global, other));
}

#[test]
fn object_cleanup_during_dispatch_skips_remaining_callbacks() {
    let observers = Observers::<u64, Vec<u32>>::default();
    let bus = observers.clone();
    let first = observers.observe(1, move |context| {
        context.push(1);
        bus.remove_key(&1);
    });
    let second = observers.observe(1, |context| context.push(2));
    let mut context = Vec::new();
    observers.notify(1, &mut context);
    observers.notify(1, &mut context);
    assert_eq!(context, [1]);
    drop((first, second));
}

#[test]
fn object_cleanup_drops_callback_captures_without_a_registry_borrow() {
    let observers = Observers::<u64>::default();
    let other = observers.observe(2, |_| {});
    observers
        .observe(1, move |_| {
            let _ = &other;
        })
        .detach();
    observers.remove_key(&1);
    observers.notify(2, &mut ());
}
