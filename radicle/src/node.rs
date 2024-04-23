#![allow(clippy::type_complexity)]
#![allow(clippy::collapsible_if)]
mod features;

pub mod address;
pub mod config;
pub mod db;
pub mod events;
pub mod notifications;
pub mod policy;
pub mod refs;
pub mod routing;
pub mod seed;
pub mod timestamp;

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader};
use std::ops::{ControlFlow, Deref};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{fmt, io, net, thread, time};

use amplify::WrapperMut;
use cyphernet::addr::NetAddr;
use localtime::{LocalDuration, LocalTime};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json as json;

use crate::crypto::PublicKey;
use crate::git;
use crate::identity::RepoId;
use crate::profile;
use crate::storage::refs::RefsAt;
use crate::storage::RefUpdate;

pub use address::KnownAddress;
pub use config::Config;
pub use cyphernet::addr::{HostName, PeerAddr};
pub use db::Database;
pub use events::{Event, Events};
pub use features::Features;
pub use seed::SyncedAt;
pub use timestamp::Timestamp;

/// Default name for control socket file.
pub const DEFAULT_SOCKET_NAME: &str = "control.sock";
/// Default radicle protocol port.
pub const DEFAULT_PORT: u16 = 8776;
/// Default timeout when waiting for the node to respond with data.
pub const DEFAULT_TIMEOUT: time::Duration = time::Duration::from_secs(30);
/// Maximum length in bytes of a node alias.
pub const MAX_ALIAS_LENGTH: usize = 32;
/// Penalty threshold at which point we avoid connecting to this node.
pub const PENALTY_THRESHOLD: u8 = 32;
/// Filename of node database under the node directory.
pub const NODE_DB_FILE: &str = "node.db";
/// Filename of policies database under the node directory.
pub const POLICIES_DB_FILE: &str = "policies.db";
/// Filename of notifications database under the node directory.
pub const NOTIFICATIONS_DB_FILE: &str = "notifications.db";
/// Filename of last node announcement, when running in debug mode.
#[cfg(debug_assertions)]
pub const NODE_ANNOUNCEMENT_FILE: &str = "announcement.wire.debug";
/// Filename of last node announcement.
#[cfg(not(debug_assertions))]
pub const NODE_ANNOUNCEMENT_FILE: &str = "announcement.wire";

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub enum PingState {
    #[default]
    /// The peer has not been sent a ping.
    None,
    /// A ping has been sent and is waiting on the peer's response.
    AwaitingResponse {
        /// Length of pong payload expected.
        len: u16,
        /// Since when are we waiting.
        since: LocalTime,
    },
    /// The peer was successfully pinged.
    Ok,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// Initial state for outgoing connections.
    Initial,
    /// Connection attempted successfully.
    Attempted,
    /// Initial state after handshake protocol hand-off.
    #[serde(rename_all = "camelCase")]
    Connected {
        /// Connected since this time.
        #[serde(with = "crate::serde_ext::localtime::time")]
        since: LocalTime,
        /// Ping state.
        #[serde(skip)]
        ping: PingState,
        /// Ongoing fetches.
        fetching: HashSet<RepoId>,
        /// Measured latencies for this peer.
        #[serde(skip)]
        latencies: VecDeque<LocalDuration>,
    },
    /// When a peer is disconnected.
    #[serde(rename_all = "camelCase")]
    Disconnected {
        /// Since when has this peer been disconnected.
        #[serde(with = "crate::serde_ext::localtime::time")]
        since: LocalTime,
        /// When to retry the connection.
        #[serde(with = "crate::serde_ext::localtime::time")]
        retry_at: LocalTime,
    },
}

impl State {
    /// Check if this is a connected state.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initial => {
                write!(f, "initial")
            }
            Self::Attempted { .. } => {
                write!(f, "attempted")
            }
            Self::Connected { .. } => {
                write!(f, "connected")
            }
            Self::Disconnected { .. } => {
                write!(f, "disconnected")
            }
        }
    }
}

/// Severity of a peer misbehavior or a connection problem.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Severity {
    Low = 0,
    Medium = 1,
    High = 8,
}

/// Node connection penalty. Nodes with a high penalty are deprioritized as peers.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct Penalty(u8);

impl Penalty {
    /// If the penalty threshold is reached, at which point we should just avoid
    /// connecting to this node.
    pub fn is_threshold_reached(&self) -> bool {
        self.0 >= PENALTY_THRESHOLD
    }
}

/// Repository sync status for our own refs.
#[derive(Debug, PartialEq, Eq, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status")]
#[serde(rename_all = "camelCase")]
pub enum SyncStatus {
    /// We're in sync.
    #[serde(rename_all = "camelCase")]
    Synced {
        /// At what ref was the remote synced at.
        at: SyncedAt,
    },
    /// We're out of sync.
    #[serde(rename_all = "camelCase")]
    OutOfSync {
        /// Local head of our `rad/sigrefs`.
        local: SyncedAt,
        /// Remote head of our `rad/sigrefs`.
        remote: SyncedAt,
    },
}

impl Ord for SyncStatus {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Synced { at: left }, Self::Synced { at: right }) => left.cmp(right),
            (Self::Synced { at }, Self::OutOfSync { remote, .. }) => at.cmp(remote),
            (Self::OutOfSync { remote, .. }, Self::Synced { at }) => remote.cmp(at),
            (Self::OutOfSync { remote: left, .. }, Self::OutOfSync { remote: right, .. }) => {
                left.cmp(right)
            }
        }
    }
}

impl PartialOrd for SyncStatus {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Node alias.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Alias(String);

impl Alias {
    /// Create a new alias from a string. Panics if the string is not a valid alias.
    pub fn new(alias: impl ToString) -> Self {
        let alias = alias.to_string();

        match Self::from_str(&alias) {
            Ok(a) => a,
            Err(e) => panic!("Alias::new: {e}"),
        }
    }
}

impl From<Alias> for String {
    fn from(value: Alias) -> Self {
        value.0
    }
}

impl From<&NodeId> for Alias {
    fn from(nid: &NodeId) -> Self {
        Alias(nid.to_string())
    }
}

impl fmt::Display for Alias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for Alias {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for Alias {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&Alias> for [u8; 32] {
    fn from(input: &Alias) -> [u8; 32] {
        let mut alias = [0u8; 32];

        alias[..input.len()].copy_from_slice(input.as_bytes());
        alias
    }
}

#[derive(thiserror::Error, Debug)]
pub enum AliasError {
    #[error("alias cannot be empty")]
    Empty,
    #[error("alias cannot be greater than {MAX_ALIAS_LENGTH} bytes")]
    MaxBytesExceeded,
    #[error("alias cannot contain whitespace or control characters")]
    InvalidCharacter,
}

impl FromStr for Alias {
    type Err = AliasError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(AliasError::Empty);
        }
        if s.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(AliasError::InvalidCharacter);
        }
        if s.len() > MAX_ALIAS_LENGTH {
            return Err(AliasError::MaxBytesExceeded);
        }
        Ok(Self(s.to_owned()))
    }
}

impl TryFrom<String> for Alias {
    type Error = AliasError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Alias::from_str(&value)
    }
}

/// Options passed to the "connect" node command.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectOptions {
    /// Establish a persistent connection.
    pub persistent: bool,
    /// How long to wait for the connection to be established.
    pub timeout: time::Duration,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            persistent: false,
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

/// Result of a command, on the node control socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandResult<T> {
    /// Response on node socket indicating that a command was carried out successfully.
    Okay(T),
    /// Response on node socket indicating that an error occured.
    Error {
        /// The reason for the error.
        #[serde(rename = "error")]
        reason: String,
    },
}

/// A success response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Success {
    /// Whether something was updated.
    #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
    updated: bool,
}

impl CommandResult<Success> {
    /// Create an "updated" response.
    pub fn updated(updated: bool) -> Self {
        Self::Okay(Success { updated })
    }

    /// Create an "ok" response.
    pub fn ok() -> Self {
        Self::Okay(Success { updated: false })
    }
}

impl CommandResult<()> {
    /// Create an error result.
    pub fn error(err: impl std::error::Error) -> Self {
        Self::Error {
            reason: err.to_string(),
        }
    }
}

impl<T: Serialize> CommandResult<T> {
    /// Write this command result to a stream, including a terminating LF character.
    pub fn to_writer(&self, mut w: impl io::Write) -> io::Result<()> {
        json::to_writer(&mut w, self).map_err(|_| io::ErrorKind::InvalidInput)?;
        w.write_all(b"\n")
    }
}

/// Peer public protocol address.
#[derive(Wrapper, WrapperMut, Clone, Eq, PartialEq, Debug, Hash, From, Serialize, Deserialize)]
#[wrapper(Deref, Display, FromStr)]
#[wrapper_mut(DerefMut)]
pub struct Address(#[serde(with = "crate::serde_ext::string")] NetAddr<HostName>);

impl Address {
    /// Check whether this address is from the local network.
    pub fn is_local(&self) -> bool {
        match self.0.host {
            HostName::Ip(ip) => address::is_local(&ip),
            _ => false,
        }
    }

    /// Check whether this address is trusted.
    /// Returns true if the address is 127.0.0.1 or 0.0.0.0.
    pub fn is_trusted(&self) -> bool {
        match self.0.host {
            HostName::Ip(ip) => ip.is_loopback() || ip.is_unspecified(),
            _ => false,
        }
    }

    /// Check whether this address is globally routable.
    pub fn is_routable(&self) -> bool {
        match self.0.host {
            HostName::Ip(ip) => address::is_routable(&ip),
            _ => true,
        }
    }
}

impl cyphernet::addr::Host for Address {
    fn requires_proxy(&self) -> bool {
        self.0.requires_proxy()
    }
}

impl cyphernet::addr::Addr for Address {
    fn port(&self) -> u16 {
        self.0.port()
    }
}

impl From<net::SocketAddr> for Address {
    fn from(addr: net::SocketAddr) -> Self {
        Address(NetAddr {
            host: HostName::Ip(addr.ip()),
            port: addr.port(),
        })
    }
}

impl From<Address> for HostName {
    fn from(addr: Address) -> Self {
        addr.0.host
    }
}

/// Command name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Command {
    /// Announce repository references for given repository to peers.
    #[serde(rename_all = "camelCase")]
    AnnounceRefs { rid: RepoId },

    /// Announce local repositories to peers.
    #[serde(rename_all = "camelCase")]
    AnnounceInventory,

    /// Update node's inventory.
    UpdateInventory { rid: RepoId },

    /// Get the current node condiguration.
    Config,

    /// Get the node's listen addresses.
    ListenAddrs,

    /// Connect to node with the given address.
    #[serde(rename_all = "camelCase")]
    Connect {
        addr: config::ConnectAddress,
        opts: ConnectOptions,
    },

    /// Disconnect from a node.
    #[serde(rename_all = "camelCase")]
    Disconnect { nid: NodeId },

    /// Lookup seeds for the given repository in the routing table.
    #[serde(rename_all = "camelCase")]
    Seeds { rid: RepoId },

    /// Get the current peer sessions.
    Sessions,

    /// Fetch the given repository from the network.
    #[serde(rename_all = "camelCase")]
    Fetch {
        rid: RepoId,
        nid: NodeId,
        timeout: time::Duration,
    },

    /// Seed the given repository.
    #[serde(rename_all = "camelCase")]
    Seed { rid: RepoId, scope: policy::Scope },

    /// Unseed the given repository.
    #[serde(rename_all = "camelCase")]
    Unseed { rid: RepoId },

    /// Follow the given node.
    #[serde(rename_all = "camelCase")]
    Follow { nid: NodeId, alias: Option<Alias> },

    /// Unfollow the given node.
    #[serde(rename_all = "camelCase")]
    Unfollow { nid: NodeId },

    /// Get the node's status.
    Status,

    /// Get the node's NID.
    NodeId,

    /// Shutdown the node.
    Shutdown,

    /// Subscribe to events.
    Subscribe,
}

impl Command {
    /// Write this command to a stream, including a terminating LF character.
    pub fn to_writer(&self, mut w: impl io::Write) -> io::Result<()> {
        json::to_writer(&mut w, self).map_err(|_| io::ErrorKind::InvalidInput)?;
        w.write_all(b"\n")
    }
}

/// Connection link direction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Link {
    /// Outgoing connection.
    Outbound,
    /// Incoming connection.
    Inbound,
}

/// An established network connection with a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub nid: NodeId,
    pub link: Link,
    pub addr: Address,
    pub state: State,
}

impl Session {
    /// Calls [`State::is_connected`] on the session state.
    pub fn is_connected(&self) -> bool {
        self.state.is_connected()
    }
}

/// A seed for some repository, with metadata about its status.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Seed {
    /// The Node ID.
    pub nid: NodeId,
    /// Known addresses for this seed.
    pub addrs: Vec<KnownAddress>,
    /// The seed's session state, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<State>,
    /// The seed's sync status, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<SyncStatus>,
}

impl Seed {
    /// Check if this is a "connected" seed.
    pub fn is_connected(&self) -> bool {
        matches!(self.state, Some(State::Connected { .. }))
    }

    /// Check if this seed is in sync with us.
    pub fn is_synced(&self) -> bool {
        matches!(self.sync, Some(SyncStatus::Synced { .. }))
    }

    pub fn new(
        nid: NodeId,
        addrs: Vec<KnownAddress>,
        state: Option<State>,
        sync: Option<SyncStatus>,
    ) -> Self {
        Self {
            nid,
            addrs,
            state,
            sync,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
/// Represents a set of seeds with associated metadata. Uses an RNG
/// underneath, so every iteration returns a different ordering.
#[serde(into = "Vec<Seed>", from = "Vec<Seed>")]
pub struct Seeds(address::AddressBook<NodeId, Seed>);

impl Seeds {
    /// Create a new seeds list from an RNG.
    pub fn new(rng: fastrand::Rng) -> Self {
        Self(address::AddressBook::new(rng))
    }

    /// Insert a seed.
    pub fn insert(&mut self, seed: Seed) {
        self.0.insert(seed.nid, seed);
    }

    /// Check membership.
    pub fn contains(&self, nid: &NodeId) -> bool {
        self.0.contains_key(nid)
    }

    /// Number of seeds.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if there are any seeds.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Partitions the list of seeds into connected and disconnected seeds.
    /// Note that the disconnected seeds may be in a "connecting" state.
    pub fn partition(&self) -> (Vec<Seed>, Vec<Seed>) {
        self.iter().cloned().partition(|s| s.is_connected())
    }

    /// Return connected seeds.
    pub fn connected(&self) -> impl Iterator<Item = &Seed> {
        self.iter().filter(|s| s.is_connected())
    }

    /// Return all seeds.
    pub fn iter(&self) -> impl Iterator<Item = &Seed> {
        self.0.shuffled().map(|(_, v)| v)
    }

    /// Check if a seed is connected.
    pub fn is_connected(&self, nid: &NodeId) -> bool {
        self.0.get(nid).map_or(false, |s| s.is_connected())
    }

    /// Return a new seeds object with the given RNG.
    pub fn with(self, rng: fastrand::Rng) -> Self {
        Self(self.0.with(rng))
    }
}

impl From<Seeds> for Vec<Seed> {
    fn from(seeds: Seeds) -> Vec<Seed> {
        seeds.0.into_shuffled().map(|(_, v)| v).collect()
    }
}

impl From<Vec<Seed>> for Seeds {
    fn from(other: Vec<Seed>) -> Seeds {
        Seeds(address::AddressBook::from_iter(
            other.into_iter().map(|s| (s.nid, s)),
        ))
    }
}

/// Announcement result returned by [`Node::announce`].
#[derive(Debug, Default)]
pub struct AnnounceResult {
    /// Nodes that timed out.
    pub timed_out: Vec<NodeId>,
    /// Nodes that synced.
    pub synced: Vec<(NodeId, time::Duration)>,
}

impl AnnounceResult {
    /// Check if a node synced successfully.
    pub fn synced(&self, nid: &NodeId) -> Option<time::Duration> {
        self.synced
            .iter()
            .find(|(id, _)| id == nid)
            .map(|(_, time)| *time)
    }
}

/// A sync event, emitted by [`Node::announce`].
#[derive(Debug)]
pub enum AnnounceEvent {
    /// Refs were synced with the given node.
    RefsSynced {
        remote: NodeId,
        time: time::Duration,
    },
    /// Refs were announced to all given nodes.
    Announced,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum FetchResult {
    Success {
        updated: Vec<RefUpdate>,
        namespaces: HashSet<NodeId>,
        clone: bool,
    },
    // TODO: Create enum for reason.
    Failed {
        reason: String,
    },
}

impl FetchResult {
    pub fn is_success(&self) -> bool {
        matches!(self, FetchResult::Success { .. })
    }

    pub fn success(self) -> Option<(Vec<RefUpdate>, HashSet<NodeId>)> {
        match self {
            Self::Success {
                updated,
                namespaces,
                ..
            } => Some((updated, namespaces)),
            _ => None,
        }
    }

    pub fn find_updated(&self, name: &git::RefStr) -> Option<RefUpdate> {
        let updated = match self {
            Self::Success { updated, .. } => Some(updated),
            _ => None,
        }?;
        updated.iter().find(|up| up.name() == name).cloned()
    }
}

impl<S: ToString> From<Result<(Vec<RefUpdate>, HashSet<NodeId>, bool), S>> for FetchResult {
    fn from(value: Result<(Vec<RefUpdate>, HashSet<NodeId>, bool), S>) -> Self {
        match value {
            Ok((updated, namespaces, clone)) => Self::Success {
                updated,
                namespaces,
                clone,
            },
            Err(err) => Self::Failed {
                reason: err.to_string(),
            },
        }
    }
}

/// Holds multiple fetch results.
#[derive(Debug, Default)]
pub struct FetchResults(Vec<(NodeId, FetchResult)>);

impl FetchResults {
    /// Push a fetch result.
    pub fn push(&mut self, nid: NodeId, result: FetchResult) {
        self.0.push((nid, result));
    }

    /// Check if the results contains the given NID.
    pub fn contains(&self, nid: &NodeId) -> bool {
        self.0.iter().any(|(n, _)| n == nid)
    }

    /// Iterate over all fetch results.
    pub fn iter(&self) -> impl Iterator<Item = (&NodeId, &FetchResult)> {
        self.0.iter().map(|(nid, r)| (nid, r))
    }

    /// Iterate over successful fetches.
    pub fn success(&self) -> impl Iterator<Item = (&NodeId, &[RefUpdate], HashSet<NodeId>)> {
        self.0.iter().filter_map(|(nid, r)| {
            if let FetchResult::Success {
                updated,
                namespaces,
                ..
            } = r
            {
                Some((nid, updated.as_slice(), namespaces.clone()))
            } else {
                None
            }
        })
    }

    /// Iterate over failed fetches.
    pub fn failed(&self) -> impl Iterator<Item = (&NodeId, &str)> {
        self.0.iter().filter_map(|(nid, r)| {
            if let FetchResult::Failed { reason } = r {
                Some((nid, reason.as_str()))
            } else {
                None
            }
        })
    }
}

impl From<Vec<(NodeId, FetchResult)>> for FetchResults {
    fn from(value: Vec<(NodeId, FetchResult)>) -> Self {
        Self(value)
    }
}

impl Deref for FetchResults {
    type Target = [(NodeId, FetchResult)];

    fn deref(&self) -> &Self::Target {
        self.0.as_slice()
    }
}

impl IntoIterator for FetchResults {
    type Item = (NodeId, FetchResult);
    type IntoIter = std::vec::IntoIter<(NodeId, FetchResult)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Error returned by [`Handle`] functions.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("node: {0}")]
    Node(String),
    #[error("timed out reading from control socket")]
    TimedOut,
    #[error("failed to open node control socket {0:?} ({1})")]
    Connect(PathBuf, io::ErrorKind),
    #[error("command error: {reason}")]
    Command { reason: String },
    #[error("received invalid json `{response}` in response to command: {error}")]
    InvalidJson {
        response: String,
        error: json::Error,
    },
    #[error("received empty response for command")]
    EmptyResponse,
}

impl Error {
    /// Check if the error is due to the not being able to connect to the local node.
    pub fn is_connection_err(&self) -> bool {
        matches!(self, Self::Connect { .. })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum ConnectResult {
    Connected,
    Disconnected { reason: String },
}

/// A handle to send commands to the node or request information.
pub trait Handle: Clone + Sync + Send {
    /// The peer sessions type.
    type Sessions;
    /// The error returned by all methods.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Get the local Node ID.
    fn nid(&self) -> Result<NodeId, Self::Error>;
    /// Check if the node is running.
    fn is_running(&self) -> bool;
    /// Get the node's bound listen addresses.
    fn listen_addrs(&self) -> Result<Vec<net::SocketAddr>, Self::Error>;
    /// Get the current node configuration.
    fn config(&self) -> Result<config::Config, Self::Error>;
    /// Connect to a peer.
    fn connect(
        &mut self,
        node: NodeId,
        addr: Address,
        opts: ConnectOptions,
    ) -> Result<ConnectResult, Self::Error>;
    /// Disconnect from a peer.
    fn disconnect(&mut self, node: NodeId) -> Result<(), Self::Error>;
    /// Lookup the seeds of a given repository in the routing table.
    fn seeds(&mut self, id: RepoId) -> Result<Seeds, Self::Error>;
    /// Fetch a repository from the network.
    fn fetch(
        &mut self,
        id: RepoId,
        from: NodeId,
        timeout: time::Duration,
    ) -> Result<FetchResult, Self::Error>;
    /// Start seeding the given repo. May update the scope. Does nothing if the
    /// repo is already seeded.
    fn seed(&mut self, id: RepoId, scope: policy::Scope) -> Result<bool, Self::Error>;
    /// Start following the given peer.
    fn follow(&mut self, id: NodeId, alias: Option<Alias>) -> Result<bool, Self::Error>;
    /// Un-seed the given repo and delete it from storage.
    fn unseed(&mut self, id: RepoId) -> Result<bool, Self::Error>;
    /// Unfollow the given peer.
    fn unfollow(&mut self, id: NodeId) -> Result<bool, Self::Error>;
    /// Notify the service that a project has been updated, and announce local refs.
    fn announce_refs(&mut self, id: RepoId) -> Result<RefsAt, Self::Error>;
    /// Announce local inventory.
    fn announce_inventory(&mut self) -> Result<(), Self::Error>;
    /// Notify the service that our inventory was updated with the given repository.
    fn update_inventory(&mut self, rid: RepoId) -> Result<bool, Self::Error>;
    /// Ask the service to shutdown.
    fn shutdown(self) -> Result<(), Self::Error>;
    /// Query the peer session state.
    fn sessions(&self) -> Result<Self::Sessions, Self::Error>;
    /// Subscribe to node events.
    fn subscribe(
        &self,
        timeout: time::Duration,
    ) -> Result<Box<dyn Iterator<Item = Result<Event, Self::Error>>>, Self::Error>;
}

/// Public node & device identifier.
pub type NodeId = PublicKey;

/// Node controller.
#[derive(Debug, Clone)]
pub struct Node {
    socket: PathBuf,
}

impl Node {
    /// Connect to the node, via the socket at the given path.
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            socket: path.as_ref().to_path_buf(),
        }
    }

    /// Call a command on the node.
    pub fn call<T: DeserializeOwned>(
        &self,
        cmd: Command,
        timeout: time::Duration,
    ) -> Result<impl Iterator<Item = Result<T, Error>>, Error> {
        let stream = UnixStream::connect(&self.socket)
            .map_err(|e| Error::Connect(self.socket.clone(), e.kind()))?;
        cmd.to_writer(&stream)?;

        stream.set_read_timeout(Some(timeout))?;

        Ok(BufReader::new(stream).lines().map(move |l| {
            let l = l.map_err(|e| match e.kind() {
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => Error::TimedOut,
                _ => Error::Io(e),
            })?;

            let result: CommandResult<T> = json::from_str(&l).map_err(|e| Error::InvalidJson {
                response: l.clone(),
                error: e,
            })?;

            match result {
                CommandResult::Okay(result) => Ok(result),
                CommandResult::Error { reason } => Err(Error::Command { reason }),
            }
        }))
    }

    /// Announce refs of the given `rid` to the given seeds.
    /// Waits for the seeds to acknowledge the refs or times out if no acknowledgments are received
    /// within the given time.
    pub fn announce(
        &mut self,
        rid: RepoId,
        seeds: impl IntoIterator<Item = NodeId>,
        timeout: time::Duration,
        mut callback: impl FnMut(AnnounceEvent, &HashMap<PublicKey, time::Duration>) -> ControlFlow<()>,
    ) -> Result<AnnounceResult, Error> {
        let events = self.subscribe(timeout)?;
        let refs = self.announce_refs(rid)?;

        let mut unsynced = seeds.into_iter().collect::<BTreeSet<_>>();
        let mut synced = HashMap::new();
        let mut timed_out: Vec<NodeId> = Vec::new();
        let started = time::Instant::now();

        callback(AnnounceEvent::Announced, &synced);

        for e in events {
            let elapsed = started.elapsed();
            if elapsed >= timeout {
                timed_out.extend(unsynced.iter());
                break;
            }
            match e {
                Ok(Event::RefsSynced {
                    remote,
                    rid: rid_,
                    at,
                }) if rid == rid_ && refs.at == at => {
                    log::debug!(target: "radicle", "Received {e:?}");

                    unsynced.remove(&remote);
                    // We can receive synced events from nodes we didn't directly announce to,
                    // and it's possible to receive duplicates as well.
                    if synced.insert(remote, elapsed).is_none() {
                        let event = AnnounceEvent::RefsSynced {
                            remote,
                            time: elapsed,
                        };
                        if callback(event, &synced).is_break() {
                            break;
                        }
                    }
                }
                Ok(_) => {}

                Err(Error::TimedOut) => {
                    timed_out.extend(unsynced.iter());
                    break;
                }
                Err(e) => return Err(e),
            }
            if unsynced.is_empty() {
                break;
            }
        }

        Ok(AnnounceResult {
            timed_out,
            synced: synced.into_iter().collect(),
        })
    }
}

// TODO(finto): repo_policies, node_policies, and routing should all
// attempt to return iterators instead of allocating vecs.
impl Handle for Node {
    type Sessions = Vec<Session>;
    type Error = Error;

    fn nid(&self) -> Result<NodeId, Error> {
        self.call::<NodeId>(Command::NodeId, DEFAULT_TIMEOUT)?
            .next()
            .ok_or(Error::EmptyResponse)?
            .map_err(Error::from)
    }

    fn listen_addrs(&self) -> Result<Vec<net::SocketAddr>, Error> {
        self.call::<Vec<net::SocketAddr>>(Command::ListenAddrs, DEFAULT_TIMEOUT)?
            .next()
            .ok_or(Error::EmptyResponse)?
            .map_err(Error::from)
    }

    fn is_running(&self) -> bool {
        let Ok(mut lines) = self.call::<Success>(Command::Status, DEFAULT_TIMEOUT) else {
            return false;
        };
        let Some(Ok(_)) = lines.next() else {
            return false;
        };
        true
    }

    fn config(&self) -> Result<config::Config, Error> {
        self.call::<config::Config>(Command::Config, DEFAULT_TIMEOUT)?
            .next()
            .ok_or(Error::EmptyResponse)?
            .map_err(Error::from)
    }

    fn connect(
        &mut self,
        nid: NodeId,
        addr: Address,
        opts: ConnectOptions,
    ) -> Result<ConnectResult, Error> {
        let timeout = opts.timeout;
        let result = self
            .call::<ConnectResult>(
                Command::Connect {
                    addr: (nid, addr).into(),
                    opts,
                },
                timeout,
            )?
            .next()
            .ok_or(Error::EmptyResponse)??;

        Ok(result)
    }

    fn disconnect(&mut self, nid: NodeId) -> Result<(), Self::Error> {
        self.call::<ConnectResult>(Command::Disconnect { nid }, DEFAULT_TIMEOUT)?
            .next()
            .ok_or(Error::EmptyResponse)??;

        Ok(())
    }

    fn seeds(&mut self, rid: RepoId) -> Result<Seeds, Error> {
        let seeds = self
            .call::<Seeds>(Command::Seeds { rid }, DEFAULT_TIMEOUT)?
            .next()
            .ok_or(Error::EmptyResponse)??;

        Ok(seeds.with(profile::env::rng()))
    }

    fn fetch(
        &mut self,
        rid: RepoId,
        from: NodeId,
        timeout: time::Duration,
    ) -> Result<FetchResult, Error> {
        let result = self
            .call(
                Command::Fetch {
                    rid,
                    nid: from,
                    timeout,
                },
                DEFAULT_TIMEOUT.max(timeout),
            )?
            .next()
            .ok_or(Error::EmptyResponse)??;

        Ok(result)
    }

    fn follow(&mut self, nid: NodeId, alias: Option<Alias>) -> Result<bool, Error> {
        let mut line = self.call::<Success>(Command::Follow { nid, alias }, DEFAULT_TIMEOUT)?;
        let response = line.next().ok_or(Error::EmptyResponse)??;

        Ok(response.updated)
    }

    fn seed(&mut self, rid: RepoId, scope: policy::Scope) -> Result<bool, Error> {
        let mut line = self.call::<Success>(Command::Seed { rid, scope }, DEFAULT_TIMEOUT)?;
        let response = line.next().ok_or(Error::EmptyResponse)??;

        Ok(response.updated)
    }

    fn unfollow(&mut self, nid: NodeId) -> Result<bool, Error> {
        let mut line = self.call::<Success>(Command::Unfollow { nid }, DEFAULT_TIMEOUT)?;
        let response = line.next().ok_or(Error::EmptyResponse)??;

        Ok(response.updated)
    }

    fn unseed(&mut self, rid: RepoId) -> Result<bool, Error> {
        let mut line = self.call::<Success>(Command::Unseed { rid }, DEFAULT_TIMEOUT)?;
        let response = line.next().ok_or(Error::EmptyResponse {})??;

        Ok(response.updated)
    }

    fn announce_refs(&mut self, rid: RepoId) -> Result<RefsAt, Error> {
        let refs: RefsAt = self
            .call(Command::AnnounceRefs { rid }, DEFAULT_TIMEOUT)?
            .next()
            .ok_or(Error::EmptyResponse)??;

        Ok(refs)
    }

    fn announce_inventory(&mut self) -> Result<(), Error> {
        for line in self.call::<Success>(Command::AnnounceInventory, DEFAULT_TIMEOUT)? {
            line?;
        }
        Ok(())
    }

    fn update_inventory(&mut self, rid: RepoId) -> Result<bool, Error> {
        let mut line = self.call::<Success>(Command::UpdateInventory { rid }, DEFAULT_TIMEOUT)?;
        let response = line.next().ok_or(Error::EmptyResponse {})??;

        Ok(response.updated)
    }

    fn subscribe(
        &self,
        timeout: time::Duration,
    ) -> Result<Box<dyn Iterator<Item = Result<Event, Error>>>, Error> {
        let events = self.call(Command::Subscribe, timeout)?;

        Ok(Box::new(events))
    }

    fn sessions(&self) -> Result<Self::Sessions, Error> {
        let sessions = self
            .call::<Vec<Session>>(Command::Sessions, DEFAULT_TIMEOUT)?
            .next()
            .ok_or(Error::EmptyResponse {})??;

        Ok(sessions)
    }

    fn shutdown(self) -> Result<(), Error> {
        for line in self.call::<Success>(Command::Shutdown, DEFAULT_TIMEOUT)? {
            line?;
        }
        // Wait until the shutdown has completed.
        while self.is_running() {
            thread::sleep(time::Duration::from_secs(1));
        }
        Ok(())
    }
}

/// A trait for different sources which can potentially return an alias.
pub trait AliasStore {
    /// Returns alias of a `NodeId`.
    fn alias(&self, nid: &NodeId) -> Option<Alias>;
}

impl AliasStore for HashMap<NodeId, Alias> {
    fn alias(&self, nid: &NodeId) -> Option<Alias> {
        self.get(nid).map(ToOwned::to_owned)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::assert_matches;

    #[test]
    fn test_alias() {
        assert!(Alias::from_str("cloudhead").is_ok());
        assert!(Alias::from_str("cloud-head").is_ok());
        assert!(Alias::from_str("cl0ud.h3ad$__").is_ok());
        assert!(Alias::from_str("©loudhèâd").is_ok());

        assert!(Alias::from_str("").is_err());
        assert!(Alias::from_str(" ").is_err());
        assert!(Alias::from_str("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_err());
        assert!(Alias::from_str("cloud\0head").is_err());
        assert!(Alias::from_str("cloud head").is_err());
        assert!(Alias::from_str("cloudhead\n").is_err());
    }

    #[test]
    fn test_command_result() {
        #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
        struct Test {
            value: u32,
        }

        assert_eq!(json::to_string(&CommandResult::Okay(true)).unwrap(), "true");
        assert_eq!(
            json::to_string(&CommandResult::Okay(Test { value: 42 })).unwrap(),
            "{\"value\":42}"
        );
        assert_eq!(
            json::from_str::<CommandResult<Test>>("{\"value\":42}").unwrap(),
            CommandResult::Okay(Test { value: 42 })
        );
        assert_eq!(json::to_string(&CommandResult::ok()).unwrap(), "{}");
        assert_eq!(
            json::to_string(&CommandResult::updated(true)).unwrap(),
            "{\"updated\":true}"
        );
        assert_eq!(
            json::to_string(&CommandResult::error(io::Error::from(
                io::ErrorKind::NotFound
            )))
            .unwrap(),
            "{\"error\":\"entity not found\"}"
        );

        json::from_str::<CommandResult<State>>(
            &serde_json::to_string(&CommandResult::Okay(State::Connected {
                since: LocalTime::now(),
                ping: Default::default(),
                fetching: Default::default(),
                latencies: VecDeque::default(),
            }))
            .unwrap(),
        )
        .unwrap();

        assert_matches!(
            json::from_str::<CommandResult<State>>(
                r#"{"connected":{"since":1699636852107,"fetching":[]}}"#
            ),
            Ok(CommandResult::Okay(_))
        );
        assert_matches!(
            json::from_str::<CommandResult<Seeds>>(
                r#"[{"nid":"z6MksmpU5b1dS7oaqF2bHXhQi1DWy2hB7Mh9CuN7y1DN6QSz","addrs":[{"addr":"seed.radicle.xyz:8776","source":"peer","lastSuccess":1699983994234,"lastAttempt":1699983994000,"banned":false}],"state":{"connected":{"since":1699983994,"fetching":[]}}}]"#
            ),
            Ok(CommandResult::Okay(_))
        );
    }
}
