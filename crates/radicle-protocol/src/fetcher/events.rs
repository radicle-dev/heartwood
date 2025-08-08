use radicle::{node::NodeId, prelude::RepoId, storage::RefUpdate};

use super::FetchingFor;

/// Events that occur when a repository is being fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetch {
    /// The repository is already being fetched.
    AlreadyFetching { rid: RepoId, fetching: FetchingFor },
    /// The capacity of the node has been reached.
    CapacityReached,
}

/// Events that occur after a repository has been fetched.
// TODO(finto): note to self a successful fetch should mark a seed as discovered
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetched {
    /// There was no ongoing fetch for the given [`NodeId`] and [`RepoId`].
    UnexpectedResult { node: NodeId, rid: RepoId },
    /// The [`RepoId`] was fetched from the [`NodeId`] with the set of updated
    /// references.
    RefsFetched {
        node: NodeId,
        rid: RepoId,
        updated: Vec<RefUpdate>,
    },
    // TODO(finto): this needs to be used to add inventory
    /// The fetched repository was a public repository.
    PublicRepo { rid: RepoId },
}
