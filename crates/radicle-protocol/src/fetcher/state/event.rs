use std::collections::{BTreeMap, VecDeque};
use std::time;

use radicle::storage::refs::RefsAt;
use radicle_core::{NodeId, RepoId};

use super::{ActiveFetch, QueuedFetch};

/// Event returned from [`FetchState::handle`].
///
/// [`FetchState::handle`]: FetchState::handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Fetch(Fetch),
    Fetched(Fetched),
    Cancel(Cancel),
}

impl From<Cancel> for Event {
    fn from(v: Cancel) -> Self {
        Self::Cancel(v)
    }
}

impl From<Fetched> for Event {
    fn from(v: Fetched) -> Self {
        Self::Fetched(v)
    }
}

impl From<Fetch> for Event {
    fn from(v: Fetch) -> Self {
        Self::Fetch(v)
    }
}

/// Events that occur when a repository is requested to be fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetch {
    /// The fetch can be started by the caller.
    Started {
        /// The repository to be fetched.
        rid: RepoId,
        /// The node to fetch from.
        from: NodeId,
        /// The references to be fetched.
        refs_at: Vec<RefsAt>,
        /// The timeout for the fetch process.
        timeout: time::Duration,
    },
    /// The repository is already being fetched from the given node.
    AlreadyFetching {
        /// The repository being actively fetched.
        rid: RepoId,
        /// The node being fetched from.
        from: NodeId,
    },
    /// The queue for the given node is at capacity, and can no longer accept
    /// any more fetch requests.
    QueueAtCapacity {
        /// The rejected repository.
        rid: RepoId,
        /// The node who's queue is at capacity.
        from: NodeId,
        /// The references expected to be fetched.
        refs_at: Vec<RefsAt>,
        /// The timeout for the fetch process.
        timeout: time::Duration,
        /// The capacity of the queue.
        capacity: usize,
    },
    /// The fetch was queued for later processing.
    Queued {
        /// The repository to be fetched.
        rid: RepoId,
        /// The node to fetch from.
        from: NodeId,
    },
}

/// Events that occur after a repository has been fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetched {
    /// There was no ongoing fetch for the given [`NodeId`] and [`RepoId`].
    NotFound { from: NodeId, rid: RepoId },
    /// The active fetch was marked as completed and removed from the active
    /// set.
    Completed {
        /// The node the repository was fetched from.
        from: NodeId,
        /// The repository that was fetched.
        rid: RepoId,
        /// The references that were fetched.
        refs_at: Vec<RefsAt>,
    },
}

/// Events that occur when a fetch was canceled for a given node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cancel {
    /// There were no active or queued fetches for the given node.
    Unexpected { from: NodeId },
    /// The were active or queued fetches that were canceled for the given node.
    Canceled {
        /// The node which was canceled.
        from: NodeId,
        /// The active fetches that were canceled.
        active: BTreeMap<RepoId, ActiveFetch>,
        /// The queued fetched that were canceled.
        queued: VecDeque<QueuedFetch>,
    },
}
