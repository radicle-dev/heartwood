use radicle_core::{NodeId, RepoId};

use crate::fetcher::RefsToFetch;

use super::FetchConfig;

/// Commands for transitioning the [`FetcherState`].
///
/// [`FetcherState`]: super::FetcherState
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Fetch(Fetch),
    Fetched(Fetched),
    Cancel(Cancel),
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

impl From<Cancel> for Command {
    fn from(v: Cancel) -> Self {
        Self::Cancel(v)
    }
}

impl Command {
    pub fn fetch(from: NodeId, rid: RepoId, refs: RefsToFetch, config: FetchConfig) -> Self {
        Self::from(Fetch {
            from,
            rid,
            refs,
            config,
        })
    }

    pub fn fetched(from: NodeId, rid: RepoId) -> Self {
        Self::from(Fetched { from, rid })
    }

    pub fn cancel(from: NodeId) -> Self {
        Self::from(Cancel { from })
    }
}

/// A fetch wants to be marked as active.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetch {
    /// The node from which the repository is being fetched from.
    pub from: NodeId,
    /// The repository to fetch.
    pub rid: RepoId,
    /// The references to fetch.
    pub refs: RefsToFetch,
    /// The configuration options for the fetch process.
    pub config: FetchConfig,
}

/// A fetch wants to be marked as completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fetched {
    /// The node from which the repository was fetched from.
    pub from: NodeId,
    /// The repository that was fetched.
    pub rid: RepoId,
}

/// Any fetches are canceled for the given node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cancel {
    /// The node for which the fetches should be canceled.
    pub from: NodeId,
}
