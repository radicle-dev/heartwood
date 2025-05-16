//! A sans-IO fetching state machine for driving fetch processes.
//!
//! See the documentation of [`Fetcher`] for more details.
pub mod fetch;
pub use fetch::{Fetcher, FetcherConfig, FetcherResult};

/// The replication factor of a syncing operation.
///
/// The factor has a lower bound, which can be considered as a part-way success
/// if reached.
///
/// The factor's upper bound is the target that the syncing operation should try
/// to reach for the operation to be considered a success.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Replicas {
    min: usize,
    max: usize,
}

impl Default for Replicas {
    fn default() -> Self {
        Self::must_reach(3)
    }
}

impl Replicas {
    /// Construct a replication factor with the `min` and `max` bounds.
    ///
    /// Ensures that `min <= max` upon construction.
    pub fn new(min: usize, max: usize) -> Self {
        let max = if min > max { min } else { max };
        Self { min, max }
    }

    /// Construct a replication factor where the `min` and `max` bounds are
    /// equal, and thus the operation must reach this factor.
    pub fn must_reach(bound: usize) -> Self {
        Self::new(bound, bound)
    }

    /// Get the minimum of the range.
    pub fn min(&self) -> usize {
        self.min
    }

    /// Get the maximum of the range.
    pub fn max(&self) -> usize {
        self.max
    }

    /// Constrain the `Replicas` maximum to the new max value.
    pub fn constrain_to(self, new_max: usize) -> Self {
        Self {
            max: self.max.min(new_max),
            ..self
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ensure_replicas_construction() {
        let replicas = Replicas::new(1, 3);
        assert!(replicas.min() <= replicas.max());
        let replicas = Replicas::new(1, 1);
        assert!(replicas.min() <= replicas.max());
        let replicas = Replicas::new(3, 1);
        assert!(replicas.min() <= replicas.max());
    }
}
