use std::{collections::HashSet, fmt, sync::Arc, time};

use crossbeam_channel::Sender;
use radicle::crypto::PublicKey;
use radicle::node::policy::Scope;
use radicle::node::FetchResult;
use radicle::node::Seeds;
use radicle::node::{Address, Alias, Config, ConnectOptions};
use radicle::storage::refs::RefsAt;
use radicle_core::{NodeId, RepoId};
use thiserror::Error;

use super::ServiceState;

/// Function used to query internal service state.
pub type QueryState = dyn Fn(&dyn ServiceState) -> Result<()> + Send + Sync;

/// A result returned from processing a [`Command`].
///
/// It is a type synonym for a [`std::result::Result`]
pub type Result<T> = std::result::Result<T, Error>;

/// Commands sent to the service by the operator.
pub enum Command {
    /// Announce repository references for given repository and namespaces to peers.
    AnnounceRefs(RepoId, HashSet<PublicKey>, Sender<RefsAt>),
    /// Announce local repositories to peers.
    AnnounceInventory,
    /// Add repository to local inventory.
    AddInventory(RepoId, Sender<bool>),
    /// Connect to node with the given address.
    Connect(NodeId, Address, ConnectOptions),
    /// Disconnect from node.
    Disconnect(NodeId),
    /// Get the node configuration.
    Config(Sender<Config>),
    /// Get the node's listen addresses.
    ListenAddrs(Sender<Vec<std::net::SocketAddr>>),
    /// Lookup seeds for the given repository in the routing table, and report
    /// sync status for given namespaces.
    Seeds(RepoId, HashSet<PublicKey>, Sender<Seeds>),
    /// Fetch the given repository from the network.
    Fetch(RepoId, NodeId, time::Duration, Sender<FetchResult>),
    /// Seed the given repository.
    Seed(RepoId, Scope, Sender<bool>),
    /// Unseed the given repository.
    Unseed(RepoId, Sender<bool>),
    /// Follow the given node.
    Follow(NodeId, Option<Alias>, Sender<bool>),
    /// Unfollow the given node.
    Unfollow(NodeId, Sender<bool>),
    /// Query the internal service state.
    QueryState(Arc<QueryState>, Sender<Result<()>>),
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnnounceRefs(id, _, _) => write!(f, "AnnounceRefs({id})"),
            Self::AnnounceInventory => write!(f, "AnnounceInventory"),
            Self::AddInventory(rid, _) => write!(f, "AddInventory({rid})"),
            Self::Connect(id, addr, opts) => write!(f, "Connect({id}, {addr}, {opts:?})"),
            Self::Disconnect(id) => write!(f, "Disconnect({id})"),
            Self::Config(_) => write!(f, "Config"),
            Self::ListenAddrs(_) => write!(f, "ListenAddrs"),
            Self::Seeds(id, _, _) => write!(f, "Seeds({id})"),
            Self::Fetch(id, node, _, _) => write!(f, "Fetch({id}, {node})"),
            Self::Seed(id, scope, _) => write!(f, "Seed({id}, {scope})"),
            Self::Unseed(id, _) => write!(f, "Unseed({id})"),
            Self::Follow(id, _, _) => write!(f, "Follow({id})"),
            Self::Unfollow(id, _) => write!(f, "Unfollow({id})"),
            Self::QueryState { .. } => write!(f, "QueryState(..)"),
        }
    }
}

/// An error that occurred when processing a service [`Command`].
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Other(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl Error {
    pub(super) fn other<E>(error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Other(Box::new(error))
    }
}
