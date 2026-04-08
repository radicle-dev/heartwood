use nonempty::NonEmpty;
use radicle::storage::refs::RefsAt;
use serde::{Deserialize, Serialize};

pub mod service;
pub use service::FetcherService;

pub mod state;
pub use state::{
    ActiveFetch, Config, FetchConfig, FetcherState, MaxQueueSize, Queue, QueueIter, QueuedFetch,
};

#[cfg(test)]
mod test;

// TODO(finto): `Service::fetch_refs_at` and the use of `refs_status_of` is a
// layer above the `Fetcher` where it would perform I/O, mocked out by a trait,
// to check if there are wants and add a fetch to the Fetcher.

/// Represents references to fetch, in the context of a repository.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RefsToFetch {
    /// Indicates that all references should be fetched.
    All,
    /// Contains a non-empty collection of specific references to fetch.
    Refs(NonEmpty<RefsAt>),
}

impl RefsToFetch {
    /// Merges another `RefsToFetch` into this one, resulting in a new
    /// `RefsToFetch` that represents the combined set of references to fetch.
    /// If either `RefsToFetch` is `All`, the result will be `All`. If both are
    /// `Refs`, their contents will be combined into a single `Refs` variant.
    pub(super) fn merge(self, other: RefsToFetch) -> Self {
        match (self, other) {
            (RefsToFetch::All, _) | (_, RefsToFetch::All) => RefsToFetch::All,
            (RefsToFetch::Refs(mut ours), RefsToFetch::Refs(theirs)) => {
                ours.extend(theirs);
                RefsToFetch::Refs(ours)
            }
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> Option<std::num::NonZeroUsize> {
        match self {
            RefsToFetch::All => None,
            RefsToFetch::Refs(refs) => std::num::NonZeroUsize::new(refs.len()),
        }
    }
}

impl From<RefsToFetch> for Vec<RefsAt> {
    fn from(val: RefsToFetch) -> Self {
        match val {
            RefsToFetch::All => Vec::new(),
            RefsToFetch::Refs(refs) => refs.into(),
        }
    }
}

impl From<Vec<RefsAt>> for RefsToFetch {
    fn from(refs_at: Vec<RefsAt>) -> Self {
        match NonEmpty::from_vec(refs_at) {
            Some(refs) => RefsToFetch::Refs(refs),
            None => RefsToFetch::All,
        }
    }
}
