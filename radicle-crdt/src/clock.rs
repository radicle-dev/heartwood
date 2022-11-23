use serde::{Deserialize, Serialize};

use crate::ord::Max;

/// Lamport clock.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LClock {
    counter: Max<u64>,
}

impl LClock {
    /// Return the clock value.
    pub fn get(&self) -> u64 {
        *self.counter.get()
    }

    /// Increment clock and return new value.
    /// Must be called before sending a message.
    pub fn tick(&mut self) -> Self {
        self.counter.incr();
        *self
    }

    /// Merge clock with another clock, and increment value.
    /// Must be called whenever a message is received.
    pub fn merge(&mut self, other: Self) -> Self {
        self.counter.merge(other.counter);
        self.tick()
    }
}

impl From<u64> for LClock {
    fn from(counter: u64) -> Self {
        Self {
            counter: Max::from(counter),
        }
    }
}
