pub mod announce;
pub use announce::{Announcer, AnnouncerConfig, AnnouncerError, AnnouncerResult};

pub mod fetch;
pub use fetch::{Fetcher, FetcherConfig, FetcherError, FetcherResult};

use std::collections::BTreeSet;

use crate::identity::Visibility;
use crate::prelude::Doc;

use super::NodeId;

/// A set of nodes that form a private network for fetching from.
///
/// This could be the set of allowed nodes for a private repository, using
/// [`PrivateNetwork::private_repo`]
pub struct PrivateNetwork {
    allowed: BTreeSet<NodeId>,
}

impl PrivateNetwork {
    pub fn private_repo(doc: &Doc) -> Option<Self> {
        match doc.visibility() {
            Visibility::Public => None,
            Visibility::Private { allow } => {
                let allowed = doc
                    .delegates()
                    .iter()
                    .chain(allow.iter())
                    .map(|did| *did.as_key())
                    .collect();
                Some(Self { allowed })
            }
        }
    }

    /// Restrict the set of allowed nodes based on the `predicate`, where `true`
    /// keeps the `NodeId` in the allowed set.
    ///
    /// For example, this can be useful to restrict the set to only connected
    /// nodes.
    pub fn restrict<P>(self, predicate: P) -> Self
    where
        P: FnMut(&NodeId) -> bool,
    {
        Self {
            allowed: self.allowed.into_iter().filter(predicate).collect(),
        }
    }
}

/// The replication factor of a syncing operation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ReplicationFactor {
    /// The syncing operation much reach the given value.
    ///
    /// See [`ReplicationFactor::must_reach`].
    MustReach(usize),
    /// The syncing operation must reach a minimum value, but may continue to
    /// reach a maximum value.
    ///
    /// See [`ReplicationFactor::range`].
    Range(ReplicationRange),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ReplicationRange {
    lower: usize,
    upper: usize,
}

impl Default for ReplicationFactor {
    fn default() -> Self {
        Self::must_reach(3)
    }
}

impl ReplicationFactor {
    /// Construct a replication factor with the `lower` and `upper` bounds.
    ///
    /// If `lower >= upper`, then [`ReplicationFactor::MustReach`] is constructed instead of
    /// `ReplicationFactor::Range`.
    pub fn range(lower: usize, upper: usize) -> Self {
        if lower >= upper {
            Self::MustReach(lower)
        } else {
            Self::Range(ReplicationRange { lower, upper })
        }
    }

    /// Construct a replication factor where the `factor` must be reached.
    pub fn must_reach(factor: usize) -> Self {
        Self::MustReach(factor)
    }

    /// Get the lower bound of the replication factor.
    pub fn lower_bound(&self) -> usize {
        match self {
            Self::MustReach(lower) => *lower,
            Self::Range(ReplicationRange { lower: min, .. }) => *min,
        }
    }

    /// Get the upper bound of the replication factor, if the replication factor
    /// is a range.
    pub fn upper_bound(&self) -> Option<usize> {
        match self {
            Self::MustReach(_) => None,
            Self::Range(ReplicationRange { upper: max, .. }) => Some(*max),
        }
    }

    /// Set the minimum target of the [`ReplicationFactor`] to a new value.
    ///
    /// If the original value is smaller than the new value, then the original
    /// is kept.
    ///
    /// If the [`ReplicationFactor`] is a range, it performs `min` on the upper
    /// bound of the range.
    ///
    /// If `self` was originally a [`ReplicationFactor::Range`], and `min >= max`, then
    /// the returned value will be [`ReplicationFactor::MustReach`].
    pub fn min(self, new: usize) -> Self {
        match self {
            Self::MustReach(min) => Self::MustReach(min.min(new)),
            Self::Range(ReplicationRange {
                lower: min,
                upper: max,
            }) => Self::range(min, max.min(new)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ensure_replicas_construction() {
        let replicas = ReplicationFactor::range(1, 3);
        assert!(
            replicas.lower_bound()
                <= replicas
                    .upper_bound()
                    .expect("replicas should have max value")
        );
        let replicas = ReplicationFactor::range(1, 1);
        assert!(replicas.upper_bound().is_none());
        let replicas = ReplicationFactor::range(3, 1);
        assert!(replicas.upper_bound().is_none());
    }

    #[test]
    fn replicas_constrain_to() {
        let replicas = ReplicationFactor::must_reach(3).min(1);
        assert_eq!(replicas, ReplicationFactor::MustReach(1));
        let replicas = ReplicationFactor::must_reach(3).min(3);
        assert_eq!(replicas, ReplicationFactor::MustReach(3));
        let replicas = ReplicationFactor::must_reach(3).min(10);
        assert_eq!(replicas, ReplicationFactor::MustReach(3));

        let replicas = ReplicationFactor::range(1, 3).min(1);
        assert_eq!(replicas, ReplicationFactor::MustReach(1));
        let replicas = ReplicationFactor::range(1, 3).min(3);
        assert_eq!(
            replicas,
            ReplicationFactor::Range(ReplicationRange { lower: 1, upper: 3 })
        );
        let replicas = ReplicationFactor::range(1, 3).min(10);
        assert_eq!(
            replicas,
            ReplicationFactor::Range(ReplicationRange { lower: 1, upper: 3 })
        );
    }
}
