use std::collections::HashSet;

use radicle::{identity::DocAt, node, node::NodeId, prelude::RepoId};

/// Effects that should be performed after a repository has been fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetched {
    /// Announce that the namespaces were fetched for this repository.
    Announce {
        rid: RepoId,
        doc: DocAt,
        namespaces: HashSet<NodeId>,
    },
    /// Notify listeners about the result of the fetch.
    Notify {
        from: NodeId,
        rid: RepoId,
        result: node::FetchResult,
    },
    /// The fetch failed, due to a timeout, so the [`NodeId`] should likely be
    /// disconnected.
    Disconnect {
        node: NodeId,
        // TODO(finto): this was a FetchError type is it ok to have it just as a
        // String?
        reason: String,
    },
}
