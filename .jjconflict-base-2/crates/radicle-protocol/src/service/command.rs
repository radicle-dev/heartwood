use std::{collections::HashSet, fmt, sync::Arc, time};

use crossbeam_channel::Receiver;
use crossbeam_channel::SendError;
use crossbeam_channel::Sender;
use radicle::crypto::PublicKey;
use radicle::node::FetchResult;
use radicle::node::Seeds;
use radicle::node::policy::Scope;
use radicle::node::{Address, Alias, Config, ConnectOptions};
use radicle::storage::refs;
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

/// A [`Responder`] returns results after processing a service [`Command`].
///
/// To construct a [`Responder`], use [`Responder::oneshot`], which also returns its
/// corresponding [`Receiver`].
///
/// To send results, use either:
/// - [`Responder::send`]
/// - [`Responder::ok`]
/// - [`Responder::err`]
#[derive(Debug)]
pub struct Responder<T> {
    channel: Sender<Result<T>>,
}

impl<T> Responder<T> {
    /// Construct a new [`Responder`] and its corresponding [`Receiver`].
    pub fn oneshot() -> (Self, Receiver<Result<T>>) {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        (Self { channel: sender }, receiver)
    }

    /// Send a [`Result`] to the receiver.
    pub fn send(self, result: Result<T>) -> std::result::Result<(), SendError<Result<T>>> {
        self.channel.send(result)
    }

    /// Send a [`Result::Ok`] to the receiver.
    pub fn ok(self, value: T) -> std::result::Result<(), SendError<Result<T>>> {
        self.send(Ok(value))
    }

    /// Send a [`Result::Err`] to the receiver.
    pub fn err<E>(self, error: E) -> std::result::Result<(), SendError<Result<T>>>
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.send(Err(Error::other(error)))
    }
}

/// Commands sent to the service by the operator.
///
/// Each variant has a corresponding helper constructor, e.g. [`Command::Seed`]
/// and [`Command::seed`]. These constructors will hide the construction of the
/// [`Responder`], and return the corresponding [`Receiver`] to receive the
/// result of the command process.
///
/// If the command does not return a [`Responder`], then it will only return the
/// [`Command`] variant, e.g. [`Command::AnnounceInventory`].
pub enum Command {
    /// Announce repository references for given repository and namespaces to peers.
    AnnounceRefs(RepoId, HashSet<PublicKey>, Responder<RefsAt>),
    /// Announce local repositories to peers.
    AnnounceInventory,
    /// Add repository to local inventory.
    AddInventory(RepoId, Responder<bool>),
    /// Connect to node with the given address.
    Connect(NodeId, Address, ConnectOptions),
    /// Disconnect from node.
    Disconnect(NodeId),
    /// Get the node configuration.
    Config(Responder<Config>),
    /// Get the node's listen addresses.
    ListenAddrs(Responder<Vec<std::net::SocketAddr>>),
    /// Lookup seeds for the given repository in the routing table, and report
    /// sync status for given namespaces.
    Seeds(RepoId, HashSet<PublicKey>, Responder<Seeds>),
    /// Fetch the given repository from the network.
    Fetch(
        RepoId,
        NodeId,
        time::Duration,
        Option<refs::FeatureLevel>,
        Responder<FetchResult>,
    ),
    /// Seed the given repository.
    Seed(RepoId, Scope, Responder<bool>),
    /// Unseed the given repository.
    Unseed(RepoId, Responder<bool>),
    /// Follow the given node.
    Follow(NodeId, Option<Alias>, Responder<bool>),
    /// Unfollow the given node.
    Unfollow(NodeId, Responder<bool>),
    /// Block the given node.
    Block(NodeId, Sender<bool>),
    /// Query the internal service state.
    QueryState(Arc<QueryState>, Sender<Result<()>>),
}

impl Command {
    pub fn announce_refs(
        rid: RepoId,
        keys: HashSet<PublicKey>,
    ) -> (Self, Receiver<Result<RefsAt>>) {
        let (responder, receiver) = Responder::oneshot();
        (Self::AnnounceRefs(rid, keys, responder), receiver)
    }

    pub fn announce_inventory() -> Self {
        Self::AnnounceInventory
    }

    pub fn add_inventory(rid: RepoId) -> (Self, Receiver<Result<bool>>) {
        let (responder, receiver) = Responder::oneshot();
        (Self::AddInventory(rid, responder), receiver)
    }

    pub fn connect(node_id: NodeId, address: Address, options: ConnectOptions) -> Self {
        Self::Connect(node_id, address, options)
    }

    pub fn disconnect(node_id: NodeId) -> Self {
        Self::Disconnect(node_id)
    }

    pub fn config() -> (Self, Receiver<Result<Config>>) {
        let (responder, receiver) = Responder::oneshot();
        (Self::Config(responder), receiver)
    }

    pub fn listen_addrs() -> (Self, Receiver<Result<Vec<std::net::SocketAddr>>>) {
        let (responder, receiver) = Responder::oneshot();
        (Self::ListenAddrs(responder), receiver)
    }

    pub fn seeds(rid: RepoId, keys: HashSet<PublicKey>) -> (Self, Receiver<Result<Seeds>>) {
        let (responder, receiver) = Responder::oneshot();
        (Self::Seeds(rid, keys, responder), receiver)
    }

    pub fn fetch(
        rid: RepoId,
        node_id: NodeId,
        duration: time::Duration,
        signed_references_minimum_feature_level: Option<refs::FeatureLevel>,
    ) -> (Self, Receiver<Result<FetchResult>>) {
        let (responder, receiver) = Responder::oneshot();
        (
            Self::Fetch(
                rid,
                node_id,
                duration,
                signed_references_minimum_feature_level,
                responder,
            ),
            receiver,
        )
    }

    pub fn seed(rid: RepoId, scope: Scope) -> (Self, Receiver<Result<bool>>) {
        let (responder, receiver) = Responder::oneshot();
        (Self::Seed(rid, scope, responder), receiver)
    }

    pub fn unseed(rid: RepoId) -> (Self, Receiver<Result<bool>>) {
        let (responder, receiver) = Responder::oneshot();
        (Self::Unseed(rid, responder), receiver)
    }

    pub fn follow(node_id: NodeId, alias: Option<Alias>) -> (Self, Receiver<Result<bool>>) {
        let (responder, receiver) = Responder::oneshot();
        (Self::Follow(node_id, alias, responder), receiver)
    }

    pub fn unfollow(node_id: NodeId) -> (Self, Receiver<Result<bool>>) {
        let (responder, receiver) = Responder::oneshot();
        (Self::Unfollow(node_id, responder), receiver)
    }

    pub fn query_state(state: Arc<QueryState>, sender: Sender<Result<()>>) -> Self {
        Self::QueryState(state, sender)
    }
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
            Self::Fetch(id, node, _, feature_level, _) => match feature_level {
                Some(feature_level) => write!(f, "Fetch({id}, {node} {feature_level})"),
                None => write!(f, "Fetch({id}, {node})"),
            },
            Self::Seed(id, scope, _) => write!(f, "Seed({id}, {scope})"),
            Self::Unseed(id, _) => write!(f, "Unseed({id})"),
            Self::Follow(id, _, _) => write!(f, "Follow({id})"),
            Self::Unfollow(id, _) => write!(f, "Unfollow({id})"),
            Self::Block(id, _) => write!(f, "Block({id})"),
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

    pub(super) fn custom(message: String) -> Self {
        Self::other(Custom { message })
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
struct Custom {
    message: String,
}
