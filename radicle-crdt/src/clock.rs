use serde::{Deserialize, Serialize};

use crate::ord::Max;
use crate::Semilattice as _;

/// Lamport clock.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Lamport {
    counter: Max<u64>,
}

impl Lamport {
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

impl From<u64> for Lamport {
    fn from(counter: u64) -> Self {
        Self {
            counter: Max::from(counter),
        }
    }
}
