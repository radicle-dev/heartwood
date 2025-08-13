//! An [`Emitter`] captures the ability to emit [`Event`]s to some subscriber
//! mechanism.

use radicle::node::Event;

use super::Events;

/// The ability of emit an event to some subscriber mechanism.
pub trait Emitter {
    /// Emit a single [`Event`], bypassing the need of an events queue.
    fn emit(&self, event: Event);

    /// Emit the next event from the events queue.
    fn emit_next(&self, events: &mut Events) {
        if let Some(event) = events.pop_event() {
            self.emit(event);
        }
    }

    /// Emit all the events that are currently on the queue.
    fn emit_all(&self, events: &mut Events) {
        for event in events.drain_events() {
            self.emit(event);
        }
    }
}
