use std::{cell::Cell, rc::Rc};

use cxstd::ecs::Events;

#[test]
fn readers_track_independent_progress_without_cloning_payloads() {
    struct Payload(String);

    let mut events = Events::default();
    let mut first = events.get_cursor();
    let mut second = events.get_cursor();
    events.write_batch([Payload("first".into()), Payload("second".into())]);

    assert_eq!(
        first
            .read(&events)
            .map(|event| event.0.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert!(first.is_empty(&events));
    assert_eq!(second.len(&events), 2);
    assert_eq!(
        second
            .read(&events)
            .map(|event| event.0.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(events.len(), 2);
}

#[test]
fn partial_iteration_advances_only_for_yielded_events() {
    let mut events = Events::default();
    events.write_batch([1, 2]);
    events.update();
    events.write_batch([3, 4]);
    let mut cursor = events.get_cursor();

    {
        let mut read = cursor.read(&events);
        assert_eq!(read.len(), 4);
        assert_eq!(read.next(), Some(&1));
        assert_eq!(read.len(), 3);
    }
    assert_eq!(cursor.len(&events), 3);
    assert_eq!(cursor.read(&events).copied().collect::<Vec<_>>(), [2, 3, 4]);
    assert!(cursor.is_empty(&events));
}

#[test]
fn rotation_expires_old_events_and_reports_missed_events() {
    let mut events = Events::default();
    let mut reader = events.get_cursor();
    let mut slow_reader = events.get_cursor();
    events.write_batch([1, 2]);
    events.update();
    events.write(3);
    assert_eq!(reader.read(&events).copied().collect::<Vec<_>>(), [1, 2, 3]);

    events.update();
    events.write(4);
    assert_eq!(slow_reader.missed_events(&events), 2);
    assert_eq!(slow_reader.len(&events), 2);
    assert_eq!(
        slow_reader.read(&events).copied().collect::<Vec<_>>(),
        [3, 4]
    );
    assert_eq!(slow_reader.missed_events(&events), 0);
    assert_eq!(reader.read(&events).copied().collect::<Vec<_>>(), [4]);
}

#[test]
fn current_cursors_ignore_existing_events_and_clone_their_position() {
    let mut events = Events::default();
    events.write(1);
    let mut cursor = events.get_cursor_current();
    assert!(cursor.is_empty(&events));
    events.write(2);
    let mut copied = cursor.clone();

    assert_eq!(cursor.read(&events).copied().collect::<Vec<_>>(), [2]);
    assert_eq!(copied.read(&events).copied().collect::<Vec<_>>(), [2]);
    events.write(3);
    cursor.clear(&events);
    assert!(cursor.is_empty(&events));
    assert_eq!(copied.read(&events).copied().collect::<Vec<_>>(), [3]);
}

#[test]
fn clearing_preserves_counts_and_allows_existing_cursors_to_continue() {
    let mut events = Events::default();
    let mut cursor = events.get_cursor();
    events.write_batch([1, 2]);
    assert_eq!(cursor.read(&events).next(), Some(&1));
    events.clear();
    assert!(events.is_empty());
    assert_eq!(cursor.missed_events(&events), 1);
    events.write(3);
    assert_eq!(cursor.read(&events).copied().collect::<Vec<_>>(), [3]);
}

#[test]
fn rotation_releases_local_payloads_without_thread_safety_bounds() {
    struct Payload(Rc<Cell<u32>>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let dropped = Rc::new(Cell::new(0));
    let mut events = Events::default();
    events.write(Payload(dropped.clone()));
    events.update();
    assert_eq!(dropped.get(), 0);
    events.update();
    assert_eq!(dropped.get(), 1);
}

#[test]
fn owned_stream_can_move_to_another_thread() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Events<String>>();

    let mut events = Events::default();
    let mut reader = events.get_cursor();
    events.write(String::from("worker result"));
    let result = std::thread::spawn(move || {
        reader
            .read(&events)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(",")
    })
    .join()
    .unwrap();
    assert_eq!(result, "worker result");
}
