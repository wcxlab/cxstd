use super::EventCursor;

/// An owned, typed event stream retaining the current and previous update batches.
///
/// Readers borrow events, so payloads need not implement `Clone`. Updates and writes
/// require exclusive access. Thread safety follows the payload's own `Send` and
/// `Sync` implementations; this type performs no synchronization.
pub struct Events<E> {
    pub(super) older: Vec<E>,
    pub(super) newer: Vec<E>,
    pub(super) event_count: usize,
}

impl<E> Events<E> {
    /// Appends an event without invoking any observers.
    pub fn write(&mut self, event: E) {
        self.newer.push(event);
        self.event_count += 1;
    }

    /// Appends events in iterator order.
    pub fn write_batch(&mut self, events: impl IntoIterator<Item = E>) {
        for event in events {
            self.write(event);
        }
    }

    /// Creates a cursor that includes all events still retained in this buffer.
    pub fn get_cursor(&self) -> EventCursor<E> {
        EventCursor::default()
    }

    /// Creates a cursor that starts after all events already written.
    pub fn get_cursor_current(&self) -> EventCursor<E> {
        let mut cursor = EventCursor::default();
        cursor.clear(self);
        cursor
    }

    /// Rotates buffers and drops the oldest batch, typically once per host update.
    ///
    /// Newly written events survive one call and expire on the second. Without
    /// updates or explicit clearing, the buffer keeps growing.
    pub fn update(&mut self) {
        std::mem::swap(&mut self.older, &mut self.newer);
        self.newer.clear();
    }

    /// Drops all retained events while preserving cursor progress and write counts.
    pub fn clear(&mut self) {
        self.older.clear();
        self.newer.clear();
    }

    /// Returns the total number of retained events, including those already read.
    pub fn len(&self) -> usize {
        self.older.len() + self.newer.len()
    }

    /// Returns whether the buffer contains no retained events.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<E> Default for Events<E> {
    fn default() -> Self {
        Self {
            older: Vec::new(),
            newer: Vec::new(),
            event_count: 0,
        }
    }
}
