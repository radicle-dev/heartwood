pub mod fetch;

pub use fetch::{Fetcher, FetcherConfig, FetcherError, FetcherResult};

/// The replication factor of a syncing operation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Replicas {
    /// The syncing operation much reach the given value.
    ///
    /// See [`Replicas::must_reach`].
    MustReach(usize),
    /// The syncing operation must reach a minimum value, but may continue to
    /// reach a maximum value.
    ///
    /// See [`Replicas::range`].
    Range(ReplicaRange),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ReplicaRange {
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
    /// If `min >= max`, then [`Replicas::MustReach`] is constructed instead of
    /// `Replicas::Range`.
    pub fn range(min: usize, max: usize) -> Self {
        if min >= max {
            Self::MustReach(min)
        } else {
            Self::Range(ReplicaRange { min, max })
        }
    }

    /// Construct a replication factor where the `min` must be reached.
    pub fn must_reach(min: usize) -> Self {
        Self::MustReach(min)
    }

    /// Get the minimum value of the replication factor.
    pub fn min(&self) -> usize {
        match self {
            Self::MustReach(min) => *min,
            Self::Range(ReplicaRange { min, .. }) => *min,
        }
    }

    /// Get the maximum of the replication factor, if the replication factor is
    /// a range.
    pub fn max(&self) -> Option<usize> {
        match self {
            Self::MustReach(_) => None,
            Self::Range(ReplicaRange { max, .. }) => Some(*max),
        }
    }

    /// Constrain the `Replicas` to a new value.
    ///
    /// If `self` was originally a [`Replicas::Range`], and `min >= max`, then
    /// the returned value will be [`Replicas::MustReach`].
    pub fn constrain_to(self, new: usize) -> Self {
        match self {
            Self::MustReach(min) => Self::MustReach(min.min(new)),
            Self::Range(ReplicaRange { min, max }) => Self::range(min, max.min(new)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ensure_replicas_construction() {
        let replicas = Replicas::range(1, 3);
        assert!(replicas.min() <= replicas.max().expect("replicas should have max value"));
        let replicas = Replicas::range(1, 1);
        assert!(replicas.max().is_none());
        let replicas = Replicas::range(3, 1);
        assert!(replicas.max().is_none());
    }

    #[test]
    fn replicas_constrain_to() {
        let replicas = Replicas::must_reach(3).constrain_to(1);
        assert_eq!(replicas, Replicas::MustReach(1));
        let replicas = Replicas::must_reach(3).constrain_to(3);
        assert_eq!(replicas, Replicas::MustReach(3));
        let replicas = Replicas::must_reach(3).constrain_to(10);
        assert_eq!(replicas, Replicas::MustReach(3));

        let replicas = Replicas::range(1, 3).constrain_to(1);
        assert_eq!(replicas, Replicas::MustReach(1));
        let replicas = Replicas::range(1, 3).constrain_to(3);
        assert_eq!(replicas, Replicas::Range(ReplicaRange { min: 1, max: 3 }));
        let replicas = Replicas::range(1, 3).constrain_to(10);
        assert_eq!(replicas, Replicas::Range(ReplicaRange { min: 1, max: 3 }));
    }
}
