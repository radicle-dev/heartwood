use std::time;

use nonempty::NonEmpty;
use radicle::{node::NodeId, prelude::RepoId, storage::refs::RefsAt};

/// Commands for transitioning the [`FetchState`].
///
/// [`FetchState`]: super::FetchState
pub enum Command {
    Fetch(Fetch),
    Fetched(Fetched),
}

impl From<Fetch> for Command {
    fn from(v: Fetch) -> Self {
        Self::Fetch(v)
    }
}

impl From<Fetched> for Command {
    fn from(v: Fetched) -> Self {
        Self::Fetched(v)
    }
}

/// Command results that occur when a repository is being fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetch {
    /// The repository should be queued for fetching.
    Queue {
        from: NodeId,
        rid: RepoId,
        refs_at: Vec<RefsAt>,
        timeout: time::Duration,
    },
    /// The repository should be fetched, and we do not know the references that
    /// are required for fetching.
    Repository {
        from: NodeId,
        rid: RepoId,
        timeout: time::Duration,
    },
    /// The repository should be fetched, and only the references stated should
    /// be fetched.
    RefsAt {
        from: NodeId,
        rid: RepoId,
        refs_at: NonEmpty<RefsAt>,
        timeout: time::Duration,
    },
}

/// Command results that occur after a repository has been fetched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fetched {
    DequeueFetches,
    Fetched { from: NodeId, rid: RepoId },
}
