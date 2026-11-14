use std::{iter::FusedIterator, marker::PhantomData, slice};

use super::Events;

/// Independent read progress for one `Events<E>` buffer.
///
/// Cloning a cursor copies its current position. Reading advances only for items
/// actually yielded; dropping a partially consumed iterator leaves the rest unread.
pub struct EventCursor<E> {
    next_event: usize,
    marker: PhantomData<fn() -> E>,
}

impl<E> EventCursor<E> {
    /// Reads unread retained events in write order and skips any expired events.
    pub fn read<'a>(&'a mut self, events: &'a Events<E>) -> EventIter<'a, E> {
        let oldest = events.event_count - events.len();
        self.next_event = self.next_event.max(oldest);
        let offset = self.next_event - oldest;
        let older_offset = offset.min(events.older.len());
        let newer_offset = offset.saturating_sub(events.older.len());
        EventIter {
            older: events.older[older_offset..].iter(),
            newer: events.newer[newer_offset..].iter(),
            next_event: &mut self.next_event,
        }
    }

    /// Returns the number of unread events still retained in the buffer.
    pub fn len(&self, events: &Events<E>) -> usize {
        (events.event_count - self.next_event).min(events.len())
    }

    /// Returns whether there are no unread retained events.
    pub fn is_empty(&self, events: &Events<E>) -> bool {
        self.len(events) == 0
    }

    /// Returns the unread events lost through rotation or explicit buffer clearing.
    ///
    /// A call to `read()` acknowledges these losses and starts at the oldest
    /// retained event. Inspect this count before reading to detect missed events.
    pub fn missed_events(&self, events: &Events<E>) -> usize {
        (events.event_count - events.len()).saturating_sub(self.next_event)
    }

    /// Skips all events written so far without removing them from the buffer.
    pub fn clear(&mut self, events: &Events<E>) {
        self.next_event = events.event_count;
    }
}

impl<E> Default for EventCursor<E> {
    fn default() -> Self {
        Self {
            next_event: 0,
            marker: PhantomData,
        }
    }
}

impl<E> Clone for EventCursor<E> {
    fn clone(&self) -> Self {
        Self {
            next_event: self.next_event,
            marker: PhantomData,
        }
    }
}

/// An iterator that advances its cursor as events are yielded.
pub struct EventIter<'a, E> {
    older: slice::Iter<'a, E>,
    newer: slice::Iter<'a, E>,
    next_event: &'a mut usize,
}

impl<'a, E> Iterator for EventIter<'a, E> {
    type Item = &'a E;

    fn next(&mut self) -> Option<Self::Item> {
        let event = self.older.next().or_else(|| self.newer.next())?;
        *self.next_event += 1;
        Some(event)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.older.len() + self.newer.len();
        (len, Some(len))
    }
}

impl<E> ExactSizeIterator for EventIter<'_, E> {}
impl<E> FusedIterator for EventIter<'_, E> {}
