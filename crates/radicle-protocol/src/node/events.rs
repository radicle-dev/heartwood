pub mod emitter;

use std::collections::VecDeque;

use radicle::node::Event;

/// Keep track of [`Event`]s that occur within the rest of the protocol system.
///
/// The events are queued with [`NodeEvents::push_event`] and removed using
/// [`NodeEvents::pop_event`] and [`NodeEvents::drain_events`].
///
/// To inspect the events use [`NodeEvents::events`].
pub struct Events {
    events: VecDeque<Event>,
}

impl Extend<Event> for Events {
    fn extend<T: IntoIterator<Item = Event>>(&mut self, iter: T) {
        self.events.extend(iter);
    }
}

impl Events {
    /// Push an [`Event`] onto the events queue.
    pub fn push_event(&mut self, event: Event) {
        self.events.push_back(event);
    }

    /// Pop the next [`Event`] from the events queue.
    pub fn pop_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    /// Drain the queue of all its events.
    ///
    /// This is useful for batch processing the available events.
    pub fn drain_events(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.events.drain(0..self.events.len())
    }
}

impl Events {
    /// Get the events that are in the queue currently, without modifying the
    /// queue itself.
    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.events.iter()
    }
}
