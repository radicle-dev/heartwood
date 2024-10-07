#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::collapsible_if)]
#![warn(clippy::unwrap_used)]
pub mod filter;
pub mod gossip;
pub mod io;
pub mod limiter;
pub mod message;
pub mod session;

use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::IpAddr;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::{fmt, net, time};

use crossbeam_channel as chan;
use fastrand::Rng;
use localtime::{LocalDuration, LocalTime};
use log::*;
use nonempty::NonEmpty;

use radicle::node;
use radicle::node::address;
use radicle::node::address::Store as _;
use radicle::node::address::{AddressBook, AddressType, KnownAddress};
use radicle::node::config::PeerConfig;
use radicle::node::refs::Store as _;
use radicle::node::routing::Store as _;
use radicle::node::seed;
use radicle::node::seed::Store as _;
use radicle::node::{ConnectOptions, Penalty, Severity};
use radicle::storage::refs::SIGREFS_BRANCH;
use radicle::storage::RepositoryError;
use radicle_fetch::policy::SeedingPolicy;

use crate::crypto::{Signer, Verified};
use crate::identity::{Doc, RepoId};
use crate::node::routing;
use crate::node::routing::InsertResult;
use crate::node::{
    Address, Alias, Features, FetchResult, HostName, Seed, Seeds, SyncStatus, SyncedAt,
};
use crate::prelude::*;
use crate::runtime::Emitter;
use crate::service::gossip::Store as _;
use crate::service::message::{
    Announcement, AnnouncementMessage, Info, NodeAnnouncement, Ping, RefsAnnouncement, RefsStatus,
};
use crate::service::policy::{store::Write, Scope};
use crate::storage;
use crate::storage::{refs::RefsAt, Namespaces, ReadStorage};
use crate::worker::fetch;
use crate::worker::FetchError;
use crate::Link;
use crate::{crypto, PROTOCOL_VERSION};

pub use crate::node::events::{Event, Events};
pub use crate::node::{config::Network, Config, NodeId};
pub use crate::service::message::{Message, ZeroBytes};
pub use crate::service::session::{QueuedFetch, Session};

pub use radicle::node::policy::config as policy;

use self::io::Outbox;
use self::limiter::RateLimiter;
use self::message::InventoryAnnouncement;
use self::policy::NamespacesError;

/// How often to run the "idle" task.
pub const IDLE_INTERVAL: LocalDuration = LocalDuration::from_secs(30);
/// How often to run the "gossip" task.
pub const GOSSIP_INTERVAL: LocalDuration = LocalDuration::from_secs(6);
/// How often to run the "announce" task.
pub const ANNOUNCE_INTERVAL: LocalDuration = LocalDuration::from_mins(60);
/// How often to run the "sync" task.
pub const SYNC_INTERVAL: LocalDuration = LocalDuration::from_secs(60);
/// How often to run the "prune" task.
pub const PRUNE_INTERVAL: LocalDuration = LocalDuration::from_mins(30);
/// Duration to wait on an unresponsive peer before dropping its connection.
pub const STALE_CONNECTION_TIMEOUT: LocalDuration = LocalDuration::from_mins(2);
/// How much time should pass after a peer was last active for a *ping* to be sent.
pub const KEEP_ALIVE_DELTA: LocalDuration = LocalDuration::from_mins(1);
/// Maximum number of latency values to keep for a session.
pub const MAX_LATENCIES: usize = 16;
/// Maximum time difference between the local time, and an announcement timestamp.
pub const MAX_TIME_DELTA: LocalDuration = LocalDuration::from_mins(60);
/// Maximum attempts to connect to a peer before we give up.
pub const MAX_CONNECTION_ATTEMPTS: usize = 3;
/// How far back from the present time should we request gossip messages when connecting to a peer,
/// when we come online for the first time.
pub const INITIAL_SUBSCRIBE_BACKLOG_DELTA: LocalDuration = LocalDuration::from_mins(60 * 24);
/// When subscribing, what margin of error do we give ourselves. A igher delta means we ask for
/// messages further back than strictly necessary, to account for missed messages.
pub const SUBSCRIBE_BACKLOG_DELTA: LocalDuration = LocalDuration::from_mins(3);
/// Minimum amount of time to wait before reconnecting to a peer.
pub const MIN_RECONNECTION_DELTA: LocalDuration = LocalDuration::from_secs(3);
/// Maximum amount of time to wait before reconnecting to a peer.
pub const MAX_RECONNECTION_DELTA: LocalDuration = LocalDuration::from_mins(60);
/// Connection retry delta used for ephemeral peers that failed to connect previously.
pub const CONNECTION_RETRY_DELTA: LocalDuration = LocalDuration::from_mins(10);
/// How long to wait for a fetch to stall before aborting, default is 3s.
pub const FETCH_TIMEOUT: time::Duration = time::Duration::from_secs(3);
/// Target number of peers to maintain connections to.
pub const TARGET_OUTBOUND_PEERS: usize = 8;

/// Maximum external address limit imposed by message size limits.
pub use message::ADDRESS_LIMIT;
/// Maximum inventory limit imposed by message size limits.
pub use message::INVENTORY_LIMIT;
/// Maximum number of project git references imposed by message size limits.
pub use message::REF_REMOTE_LIMIT;

/// Metrics we track.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    /// Metrics for each peer.
    pub peers: HashMap<NodeId, PeerMetrics>,
    /// Tasks queued in worker queue.
    pub worker_queue_size: usize,
    /// Current open channel count.
    pub open_channels: usize,
}

impl Metrics {
    /// Get metrics for the given peer.
    pub fn peer(&mut self, nid: NodeId) -> &mut PeerMetrics {
        self.peers.entry(nid).or_default()
    }
}

/// Per-peer metrics we track.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerMetrics {
    pub received_git_bytes: usize,
    pub received_fetch_requests: usize,
    pub received_bytes: usize,
    pub received_gossip_messages: usize,
    pub sent_bytes: usize,
    pub sent_fetch_requests: usize,
    pub sent_git_bytes: usize,
    pub sent_gossip_messages: usize,
    pub streams_opened: usize,
    pub inbound_connection_attempts: usize,
    pub outbound_connection_attempts: usize,
    pub disconnects: usize,
}

/// Result of syncing our routing table with a node's inventory.
#[derive(Default)]
struct SyncedRouting {
    /// Repo entries added.
    added: Vec<RepoId>,
    /// Repo entries removed.
    removed: Vec<RepoId>,
    /// Repo entries updated (time).
    updated: Vec<RepoId>,
}

impl SyncedRouting {
    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.updated.is_empty()
    }
}

/// A peer we can connect to.
#[derive(Debug, Clone)]
struct Peer {
    nid: NodeId,
    addresses: Vec<KnownAddress>,
    penalty: Penalty,
}

/// General service error.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Git(#[from] radicle::git::raw::Error),
    #[error(transparent)]
    GitExt(#[from] radicle::git::ext::Error),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Gossip(#[from] gossip::Error),
    #[error(transparent)]
    Refs(#[from] storage::refs::Error),
    #[error(transparent)]
    Routing(#[from] routing::Error),
    #[error(transparent)]
    Address(#[from] address::Error),
    #[error(transparent)]
    Database(#[from] node::db::Error),
    #[error(transparent)]
    Seeds(#[from] seed::Error),
    #[error(transparent)]
    Policy(#[from] policy::Error),
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
    #[error("namespaces error: {0}")]
    Namespaces(#[from] NamespacesError),
}

/// A store for all node data.
pub trait Store:
    address::Store + gossip::Store + routing::Store + seed::Store + node::refs::Store
{
}

impl Store for node::Database {}

/// Function used to query internal service state.
pub type QueryState = dyn Fn(&dyn ServiceState) -> Result<(), CommandError> + Send + Sync;

/// Commands sent to the service by the operator.
pub enum Command {
    /// Announce repository references for given repository to peers.
    AnnounceRefs(RepoId, chan::Sender<RefsAt>),
    /// Announce local repositories to peers.
    AnnounceInventory,
    /// Add repository to local inventory.
    AddInventory(RepoId, chan::Sender<bool>),
    /// Connect to node with the given address.
    Connect(NodeId, Address, ConnectOptions),
    /// Disconnect from node.
    Disconnect(NodeId),
    /// Get the node configuration.
    Config(chan::Sender<Config>),
    /// Get the node's listen addresses.
    ListenAddrs(chan::Sender<Vec<std::net::SocketAddr>>),
    /// Lookup seeds for the given repository in the routing table.
    Seeds(RepoId, chan::Sender<Seeds>),
    /// Fetch the given repository from the network.
    Fetch(RepoId, NodeId, time::Duration, chan::Sender<FetchResult>),
    /// Seed the given repository.
    Seed(RepoId, Scope, chan::Sender<bool>),
    /// Unseed the given repository.
    Unseed(RepoId, chan::Sender<bool>),
    /// Follow the given node.
    Follow(NodeId, Option<Alias>, chan::Sender<bool>),
    /// Unfollow the given node.
    Unfollow(NodeId, chan::Sender<bool>),
    /// Query the internal service state.
    QueryState(Arc<QueryState>, chan::Sender<Result<(), CommandError>>),
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnnounceRefs(id, _) => write!(f, "AnnounceRefs({id})"),
            Self::AnnounceInventory => write!(f, "AnnounceInventory"),
            Self::AddInventory(rid, _) => write!(f, "AddInventory({rid})"),
            Self::Connect(id, addr, opts) => write!(f, "Connect({id}, {addr}, {opts:?})"),
            Self::Disconnect(id) => write!(f, "Disconnect({id})"),
            Self::Config(_) => write!(f, "Config"),
            Self::ListenAddrs(_) => write!(f, "ListenAddrs"),
            Self::Seeds(id, _) => write!(f, "Seeds({id})"),
            Self::Fetch(id, node, _, _) => write!(f, "Fetch({id}, {node})"),
            Self::Seed(id, scope, _) => write!(f, "Seed({id}, {scope})"),
            Self::Unseed(id, _) => write!(f, "Unseed({id})"),
            Self::Follow(id, _, _) => write!(f, "Follow({id})"),
            Self::Unfollow(id, _) => write!(f, "Unfollow({id})"),
            Self::QueryState { .. } => write!(f, "QueryState(..)"),
        }
    }
}

/// Command-related errors.
#[derive(thiserror::Error, Debug)]
pub enum CommandError {
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Routing(#[from] routing::Error),
    #[error(transparent)]
    Policy(#[from] policy::Error),
}

/// Error returned by [`Service::try_fetch`].
#[derive(thiserror::Error, Debug)]
enum TryFetchError<'a> {
    #[error("ongoing fetch for repository exists")]
    AlreadyFetching(&'a mut FetchState),
    #[error("peer is not connected; cannot initiate fetch")]
    SessionNotConnected,
    #[error("peer fetch capacity reached; cannot initiate fetch")]
    SessionCapacityReached,
    #[error(transparent)]
    Namespaces(#[from] NamespacesError),
}

/// Fetch state for an ongoing fetch.
#[derive(Debug)]
pub struct FetchState {
    /// Node we're fetching from.
    pub from: NodeId,
    /// What refs we're fetching.
    pub refs_at: Vec<RefsAt>,
    /// Channels waiting for fetch results.
    pub subscribers: Vec<chan::Sender<FetchResult>>,
}

impl FetchState {
    /// Add a subscriber to this fetch.
    fn subscribe(&mut self, c: chan::Sender<FetchResult>) {
        if !self.subscribers.iter().any(|s| s.same_channel(&c)) {
            self.subscribers.push(c);
        }
    }
}

/// Holds all node stores.
#[derive(Debug)]
pub struct Stores<D>(D);

impl<D> Stores<D>
where
    D: Store,
{
    /// Get the database as a routing store.
    pub fn routing(&self) -> &impl routing::Store {
        &self.0
    }

    /// Get the database as a routing store, mutably.
    pub fn routing_mut(&mut self) -> &mut impl routing::Store {
        &mut self.0
    }

    /// Get the database as an address store.
    pub fn addresses(&self) -> &impl address::Store {
        &self.0
    }

    /// Get the database as an address store, mutably.
    pub fn addresses_mut(&mut self) -> &mut impl address::Store {
        &mut self.0
    }

    /// Get the database as a gossip store.
    pub fn gossip(&self) -> &impl gossip::Store {
        &self.0
    }

    /// Get the database as a gossip store, mutably.
    pub fn gossip_mut(&mut self) -> &mut impl gossip::Store {
        &mut self.0
    }

    /// Get the database as a seed store.
    pub fn seeds(&self) -> &impl seed::Store {
        &self.0
    }

    /// Get the database as a seed store, mutably.
    pub fn seeds_mut(&mut self) -> &mut impl seed::Store {
        &mut self.0
    }

    /// Get the database as a refs db.
    pub fn refs(&self) -> &impl node::refs::Store {
        &self.0
    }

    /// Get the database as a refs db, mutably.
    pub fn refs_mut(&mut self) -> &mut impl node::refs::Store {
        &mut self.0
    }
}

impl<D> AsMut<D> for Stores<D> {
    fn as_mut(&mut self) -> &mut D {
        &mut self.0
    }
}

impl<D> From<D> for Stores<D> {
    fn from(db: D) -> Self {
        Self(db)
    }
}

/// The node service.
#[derive(Debug)]
pub struct Service<D, S, G> {
    /// Service configuration.
    config: Config,
    /// Our cryptographic signer and key.
    signer: G,
    /// Project storage.
    storage: S,
    /// Node database.
    db: Stores<D>,
    /// Policy configuration.
    policies: policy::Config<Write>,
    /// Peer sessions, currently or recently connected.
    sessions: Sessions,
    /// Clock. Tells the time.
    clock: LocalTime,
    /// Who relayed what announcement to us. We keep track of this to ensure that
    /// we don't relay messages to nodes that already know about these messages.
    relayed_by: HashMap<gossip::AnnouncementId, Vec<NodeId>>,
    /// I/O outbox.
    outbox: Outbox,
    /// Cached local node announcement.
    node: NodeAnnouncement,
    /// Cached local inventory announcement.
    inventory: InventoryAnnouncement,
    /// Source of entropy.
    rng: Rng,
    /// Ongoing fetches.
    fetching: HashMap<RepoId, FetchState>,
    /// Request/connection rate limiter.
    limiter: RateLimiter,
    /// Current seeded repositories bloom filter.
    filter: Filter,
    /// Last time the service was idle.
    last_idle: LocalTime,
    /// Last time the gossip messages were relayed.
    last_gossip: LocalTime,
    /// Last time the service synced.
    last_sync: LocalTime,
    /// Last time the service routing table was pruned.
    last_prune: LocalTime,
    /// Last time the announcement task was run.
    last_announce: LocalTime,
    /// Timestamp of last local inventory announced.
    last_inventory: LocalTime,
    /// Last timestamp used for announcements.
    last_timestamp: Timestamp,
    /// Time when the service was initialized, or `None` if it wasn't initialized.
    started_at: Option<LocalTime>,
    /// Time when the service was last online, or `None` if this is the first time.
    last_online_at: Option<LocalTime>,
    /// Publishes events to subscribers.
    emitter: Emitter<Event>,
    /// Local listening addresses.
    listening: Vec<net::SocketAddr>,
    /// Latest metrics for all nodes connected to since the last start.
    metrics: Metrics,
}

impl<D, S, G> Service<D, S, G>
where
    G: crypto::Signer,
{
    /// Get the local node id.
    pub fn node_id(&self) -> NodeId {
        *self.signer.public_key()
    }

    /// Get the local service time.
    pub fn local_time(&self) -> LocalTime {
        self.clock
    }

    pub fn emitter(&self) -> Emitter<Event> {
        self.emitter.clone()
    }
}

impl<D, S, G> Service<D, S, G>
where
    D: Store,
    S: ReadStorage + 'static,
    G: Signer,
{
    pub fn new(
        config: Config,
        db: Stores<D>,
        storage: S,
        policies: policy::Config<Write>,
        signer: G,
        rng: Rng,
        node: NodeAnnouncement,
        emitter: Emitter<Event>,
    ) -> Self {
        let sessions = Sessions::new(rng.clone());
        let limiter = RateLimiter::new(config.peers());
        let last_timestamp = node.timestamp;
        let clock = LocalTime::default(); // Updated on initialize.
        let inventory = gossip::inventory(clock.into(), []); // Updated on initialize.

        Self {
            config,
            storage,
            policies,
            signer,
            rng,
            inventory,
            node,
            clock,
            db,
            outbox: Outbox::default(),
            limiter,
            sessions,
            fetching: HashMap::new(),
            filter: Filter::empty(),
            relayed_by: HashMap::default(),
            last_idle: LocalTime::default(),
            last_gossip: LocalTime::default(),
            last_sync: LocalTime::default(),
            last_prune: LocalTime::default(),
            last_timestamp,
            last_announce: LocalTime::default(),
            last_inventory: LocalTime::default(),
            started_at: None,     // Updated on initialize.
            last_online_at: None, // Updated on initialize.
            emitter,
            listening: vec![],
            metrics: Metrics::default(),
        }
    }

    /// Whether the service was started (initialized) and if so, at what time.
    pub fn started(&self) -> Option<LocalTime> {
        self.started_at
    }

    /// Return the next i/o action to execute.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<io::Io> {
        self.outbox.next()
    }

    /// Seed a repository.
    /// Returns whether or not the repo policy was updated.
    pub fn seed(&mut self, id: &RepoId, scope: Scope) -> Result<bool, policy::Error> {
        let updated = self.policies.seed(id, scope)?;
        self.filter.insert(id);

        Ok(updated)
    }

    /// Unseed a repository.
    /// Returns whether or not the repo policy was updated.
    /// Note that when unseeding, we don't announce anything to the network. This is because by
    /// simply not announcing it anymore, it will eventually be pruned by nodes.
    pub fn unseed(&mut self, id: &RepoId) -> Result<bool, policy::Error> {
        let updated = self.policies.unseed(id)?;

        if updated {
            // Nb. This is potentially slow if we have lots of repos. We should probably
            // only re-compute the filter when we've unseeded a certain amount of repos
            // and the filter is really out of date.
            //
            // TODO: Share this code with initialization code.
            self.filter = Filter::new(
                self.policies
                    .seed_policies()?
                    .filter_map(|t| (t.policy.is_allow()).then_some(t.rid)),
            );
            // Update and announce new inventory.
            if let Err(e) = self.remove_inventory(id) {
                error!(target: "service", "Error updating inventory after unseed: {e}");
            }
        }
        Ok(updated)
    }

    /// Find the closest `n` peers by proximity in seeding graphs.
    /// Returns a sorted list from the closest peer to the furthest.
    /// Peers with more seedings in common score score higher.
    #[allow(unused)]
    pub fn closest_peers(&self, n: usize) -> Vec<NodeId> {
        todo!()
    }

    /// Get the database.
    pub fn database(&self) -> &Stores<D> {
        &self.db
    }

    /// Get the mutable database.
    pub fn database_mut(&mut self) -> &mut Stores<D> {
        &mut self.db
    }

    /// Get the storage instance.
    pub fn storage(&self) -> &S {
        &self.storage
    }

    /// Get the mutable storage instance.
    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    /// Get the node policies.
    pub fn policies(&self) -> &policy::Config<Write> {
        &self.policies
    }

    /// Get the local signer.
    pub fn signer(&self) -> &G {
        &self.signer
    }

    /// Subscriber to inner `Emitter` events.
    pub fn events(&mut self) -> Events {
        Events::from(self.emitter.subscribe())
    }

    /// Get I/O outbox.
    pub fn outbox(&mut self) -> &mut Outbox {
        &mut self.outbox
    }

    /// Get configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Lookup a repository, both locally and in the routing table.
    pub fn lookup(&self, rid: RepoId) -> Result<Lookup, LookupError> {
        let this = self.nid();
        let local = self.storage.get(rid)?;
        let remote = self
            .db
            .routing()
            .get(&rid)?
            .iter()
            .filter(|nid| nid != &this)
            .cloned()
            .collect();

        Ok(Lookup { local, remote })
    }

    /// Initialize service with current time. Call this once.
    pub fn initialize(&mut self, time: LocalTime) -> Result<(), Error> {
        debug!(target: "service", "Init @{}", time.as_millis());
        assert_ne!(time, LocalTime::default());

        let nid = self.node_id();

        self.clock = time;
        self.started_at = Some(time);
        self.last_online_at = match self.db.gossip().last() {
            Ok(Some(last)) => Some(last.to_local_time()),
            Ok(None) => None,
            Err(e) => {
                error!(target: "service", "Error getting the lastest gossip message from db: {e}");
                None
            }
        };

        // Populate refs database. This is only useful as part of the upgrade process for nodes
        // that have been online since before the refs database was created.
        match self.db.refs().count() {
            Ok(0) => {
                info!(target: "service", "Empty refs database, populating from storage..");
                if let Err(e) = self.db.refs_mut().populate(&self.storage) {
                    error!(target: "service", "Failed to populate refs database: {e}");
                }
            }
            Ok(n) => debug!(target: "service", "Refs database has {n} cached references"),
            Err(e) => error!(target: "service", "Error checking refs database: {e}"),
        }

        let announced = self
            .db
            .seeds()
            .seeded_by(&nid)?
            .collect::<Result<HashMap<_, _>, _>>()?;
        let mut inventory = BTreeSet::new();
        let mut private = BTreeSet::new();

        for repo in self.storage.repositories()? {
            let rid = repo.rid;

            // If we're not seeding this repo, just skip it.
            if !self.policies.is_seeding(&rid)? {
                warn!(target: "service", "Local repository {rid} is not seeded");
                continue;
            }
            // Add public repositories to inventory.
            if repo.doc.visibility.is_public() {
                inventory.insert(rid);
            } else {
                private.insert(rid);
            }
            // If we have no owned refs for this repo, then there's nothing to announce.
            let Some(updated_at) = repo.synced_at else {
                continue;
            };
            // Skip this repo if the sync status matches what we have in storage.
            if let Some(announced) = announced.get(&rid) {
                if updated_at.oid == announced.oid {
                    continue;
                }
            }
            // Make sure our local node's sync status is up to date with storage.
            if self.db.seeds_mut().synced(
                &rid,
                &nid,
                updated_at.oid,
                updated_at.timestamp.into(),
            )? {
                debug!(target: "service", "Saved local sync status for {rid}..");
            }
            // If we got here, it likely means a repo was updated while the node was stopped.
            // Therefore, we pre-load a refs announcement for this repo, so that it is included in
            // the historical gossip messages when a node connects and subscribes to this repo.
            if let Ok((ann, _)) = self.refs_announcement_for(rid, [nid]) {
                debug!(target: "service", "Adding refs announcement for {rid} to historical gossip messages..");
                self.db.gossip_mut().announced(&nid, &ann)?;
            }
        }

        // Ensure that our inventory is recorded in our routing table, and we are seeding
        // all of it. It can happen that inventory is not properly seeded if for eg. the
        // user creates a new repository while the node is stopped.
        self.db
            .routing_mut()
            .add_inventory(inventory.iter(), nid, time.into())?;
        self.inventory = gossip::inventory(self.timestamp(), inventory);

        // Ensure that private repositories are not in our inventory. It's possible that
        // a repository was public and then it was made private.
        self.db
            .routing_mut()
            .remove_inventories(private.iter(), &nid)?;

        // Setup subscription filter for seeded repos.
        self.filter = Filter::new(
            self.policies
                .seed_policies()?
                .filter_map(|t| (t.policy.is_allow()).then_some(t.rid)),
        );
        // Connect to configured peers.
        let addrs = self.config.connect.clone();
        for (id, addr) in addrs.into_iter().map(|ca| ca.into()) {
            self.connect(id, addr);
        }
        // Try to establish some connections.
        self.maintain_connections();
        // Start periodic tasks.
        self.outbox.wakeup(IDLE_INTERVAL);
        self.outbox.wakeup(GOSSIP_INTERVAL);

        Ok(())
    }

    pub fn tick(&mut self, now: LocalTime, metrics: &Metrics) {
        trace!(
            target: "service",
            "Tick +{}",
            now - self.started_at.expect("Service::tick: service must be initialized")
        );
        if now >= self.clock {
            self.clock = now;
        } else {
            // Nb. In tests, we often move the clock forwards in time to test different behaviors,
            // so this warning isn't applicable there.
            #[cfg(not(test))]
            warn!(
                target: "service",
                "System clock is not monotonic: {now} is not greater or equal to {}", self.clock
            );
        }
        self.metrics = metrics.clone();
    }

    pub fn wake(&mut self) {
        let now = self.clock;

        trace!(
            target: "service",
            "Wake +{}",
            now - self.started_at.expect("Service::wake: service must be initialized")
        );

        if now - self.last_idle >= IDLE_INTERVAL {
            trace!(target: "service", "Running 'idle' task...");

            self.keep_alive(&now);
            self.disconnect_unresponsive_peers(&now);
            self.idle_connections();
            self.maintain_connections();
            self.dequeue_fetches();
            self.outbox.wakeup(IDLE_INTERVAL);
            self.last_idle = now;
        }
        if now - self.last_gossip >= GOSSIP_INTERVAL {
            trace!(target: "service", "Running 'gossip' task...");

            if let Err(e) = self.relay_announcements() {
                error!(target: "service", "Error relaying stored announcements: {e}");
            }
            self.outbox.wakeup(GOSSIP_INTERVAL);
            self.last_gossip = now;
        }
        if now - self.last_sync >= SYNC_INTERVAL {
            trace!(target: "service", "Running 'sync' task...");

            if let Err(e) = self.fetch_missing_repositories() {
                error!(target: "service", "Error fetching missing inventory: {e}");
            }
            self.outbox.wakeup(SYNC_INTERVAL);
            self.last_sync = now;
        }
        if now - self.last_announce >= ANNOUNCE_INTERVAL {
            trace!(target: "service", "Running 'announce' task...");

            self.announce_inventory();
            self.outbox.wakeup(ANNOUNCE_INTERVAL);
            self.last_announce = now;
        }
        if now - self.last_prune >= PRUNE_INTERVAL {
            trace!(target: "service", "Running 'prune' task...");

            if let Err(err) = self.prune_routing_entries(&now) {
                error!(target: "service", "Error pruning routing entries: {err}");
            }
            if let Err(err) = self
                .db
                .gossip_mut()
                .prune((now - self.config.limits.gossip_max_age).into())
            {
                error!(target: "service", "Error pruning gossip entries: {err}");
            }

            self.outbox.wakeup(PRUNE_INTERVAL);
            self.last_prune = now;
        }

        // Always check whether there are persistent peers that need reconnecting.
        self.maintain_persistent();
    }

    pub fn command(&mut self, cmd: Command) {
        info!(target: "service", "Received command {:?}", cmd);

        match cmd {
            Command::Connect(nid, addr, opts) => {
                if opts.persistent {
                    self.config.connect.insert((nid, addr.clone()).into());
                }
                if !self.connect(nid, addr) {
                    // TODO: Return error to command.
                }
            }
            Command::Disconnect(nid) => {
                self.outbox.disconnect(nid, DisconnectReason::Command);
            }
            Command::Config(resp) => {
                resp.send(self.config.clone()).ok();
            }
            Command::ListenAddrs(resp) => {
                resp.send(self.listening.clone()).ok();
            }
            Command::Seeds(rid, resp) => match self.seeds(&rid) {
                Ok(seeds) => {
                    let (connected, disconnected) = seeds.partition();
                    debug!(
                        target: "service",
                        "Found {} connected seed(s) and {} disconnected seed(s) for {}",
                        connected.len(), disconnected.len(),  rid
                    );
                    resp.send(seeds).ok();
                }
                Err(e) => {
                    error!(target: "service", "Error getting seeds for {rid}: {e}");
                }
            },
            Command::Fetch(rid, seed, timeout, resp) => {
                self.fetch(rid, seed, timeout, Some(resp));
            }
            Command::Seed(rid, scope, resp) => {
                // Update our seeding policy.
                let seeded = self
                    .seed(&rid, scope)
                    .expect("Service::command: error seeding repository");
                resp.send(seeded).ok();

                // Let all our peers know that we're interested in this repo from now on.
                self.outbox.broadcast(
                    Message::subscribe(self.filter(), self.clock.into(), Timestamp::MAX),
                    self.sessions.connected().map(|(_, s)| s),
                );
            }
            Command::Unseed(id, resp) => {
                let updated = self
                    .unseed(&id)
                    .expect("Service::command: error unseeding repository");
                resp.send(updated).ok();
            }
            Command::Follow(id, alias, resp) => {
                let seeded = self
                    .policies
                    .follow(&id, alias.as_deref())
                    .expect("Service::command: error following node");
                resp.send(seeded).ok();
            }
            Command::Unfollow(id, resp) => {
                let updated = self
                    .policies
                    .unfollow(&id)
                    .expect("Service::command: error unfollowing node");
                resp.send(updated).ok();
            }
            Command::AnnounceRefs(id, resp) => {
                let doc = match self.storage.get(id) {
                    Ok(Some(doc)) => doc,
                    Ok(None) => {
                        error!(target: "service", "Error announcing refs: repository {id} not found");
                        return;
                    }
                    Err(e) => {
                        error!(target: "service", "Error announcing refs: doc error: {e}");
                        return;
                    }
                };

                match self.announce_own_refs(id, doc) {
                    Ok(refs) => match refs.as_slice() {
                        &[refs] => {
                            resp.send(refs).ok();
                        }
                        // SAFETY: Since we passed in one NID, we should get exactly one item back.
                        [..] => panic!("Service::command: unexpected refs returned"),
                    },
                    Err(err) => {
                        error!(target: "service", "Error announcing refs: {err}");
                    }
                }
            }
            Command::AnnounceInventory => {
                self.announce_inventory();
            }
            Command::AddInventory(rid, resp) => match self.add_inventory(rid) {
                Ok(updated) => {
                    resp.send(updated).ok();
                }
                Err(e) => {
                    error!(target: "service", "Error adding {rid} to inventory: {e}");
                }
            },
            Command::QueryState(query, sender) => {
                sender.send(query(self)).ok();
            }
        }
    }

    /// Initiate an outgoing fetch for some repository, based on another node's announcement.
    /// Returns `true` if the fetch was initiated and `false` if it was skipped.
    fn fetch_refs_at(
        &mut self,
        rid: RepoId,
        from: NodeId,
        refs: NonEmpty<RefsAt>,
        scope: Scope,
        timeout: time::Duration,
        channel: Option<chan::Sender<FetchResult>>,
    ) -> bool {
        match self.refs_status_of(rid, refs, &scope) {
            Ok(status) => {
                if status.want.is_empty() {
                    debug!(target: "service", "Skipping fetch for {rid}, all refs are already in storage");
                } else {
                    return self._fetch(rid, from, status.want, timeout, channel);
                }
            }
            Err(e) => {
                error!(target: "service", "Error getting the refs status of {rid}: {e}");
            }
        }
        // We didn't try to fetch anything.
        false
    }

    /// Initiate an outgoing fetch for some repository.
    fn fetch(
        &mut self,
        rid: RepoId,
        from: NodeId,
        timeout: time::Duration,
        channel: Option<chan::Sender<FetchResult>>,
    ) -> bool {
        self._fetch(rid, from, vec![], timeout, channel)
    }

    fn _fetch(
        &mut self,
        rid: RepoId,
        from: NodeId,
        refs_at: Vec<RefsAt>,
        timeout: time::Duration,
        channel: Option<chan::Sender<FetchResult>>,
    ) -> bool {
        match self.try_fetch(rid, &from, refs_at.clone(), timeout) {
            Ok(fetching) => {
                if let Some(c) = channel {
                    fetching.subscribe(c);
                }
                return true;
            }
            Err(TryFetchError::AlreadyFetching(fetching)) => {
                // If we're already fetching the same refs from the requested peer, there's nothing
                // to do, we simply add the supplied channel to the list of subscribers so that it
                // is notified on completion. Otherwise, we queue a fetch with the requested peer.
                if fetching.from == from && fetching.refs_at == refs_at {
                    debug!(target: "service", "Ignoring redundant fetch of {rid} from {from}");

                    if let Some(c) = channel {
                        fetching.subscribe(c);
                    }
                } else {
                    let fetch = QueuedFetch {
                        rid,
                        refs_at,
                        from,
                        timeout,
                        channel,
                    };
                    debug!(target: "service", "Queueing fetch for {rid} with {from} (already fetching)..");

                    self.queue_fetch(fetch);
                }
            }
            Err(TryFetchError::SessionCapacityReached) => {
                debug!(target: "service", "Fetch capacity reached for {from}, queueing {rid}..");
                self.queue_fetch(QueuedFetch {
                    rid,
                    refs_at,
                    from,
                    timeout,
                    channel,
                });
            }
            Err(e) => {
                if let Some(c) = channel {
                    c.send(FetchResult::Failed {
                        reason: e.to_string(),
                    })
                    .ok();
                }
            }
        }
        false
    }

    fn queue_fetch(&mut self, fetch: QueuedFetch) {
        let Some(s) = self.sessions.get_mut(&fetch.from) else {
            log::error!(target: "service", "Cannot queue fetch for unknown session {}", fetch.from);
            return;
        };
        if let Err(e) = s.queue_fetch(fetch) {
            let fetch = e.inner();
            log::debug!(target: "service", "Unable to queue fetch for {} with {}: {e}", &fetch.rid, &fetch.from);
        }
    }

    // TODO: Buffer/throttle fetches.
    fn try_fetch(
        &mut self,
        rid: RepoId,
        from: &NodeId,
        refs_at: Vec<RefsAt>,
        timeout: time::Duration,
    ) -> Result<&mut FetchState, TryFetchError> {
        let from = *from;
        let Some(session) = self.sessions.get_mut(&from) else {
            return Err(TryFetchError::SessionNotConnected);
        };
        let fetching = self.fetching.entry(rid);

        trace!(target: "service", "Trying to fetch {refs_at:?} for {rid}..");

        let fetching = match fetching {
            Entry::Vacant(fetching) => fetching,
            Entry::Occupied(fetching) => {
                // We're already fetching this repo from some peer.
                return Err(TryFetchError::AlreadyFetching(fetching.into_mut()));
            }
        };
        // Sanity check: We shouldn't be fetching from this session, since we return above if we're
        // fetching from any session.
        debug_assert!(!session.is_fetching(&rid));

        if !session.is_connected() {
            // This can happen if a session disconnects in the time between asking for seeds to
            // fetch from, and initiating the fetch from one of those seeds.
            return Err(TryFetchError::SessionNotConnected);
        }
        if session.is_at_capacity() {
            // If we're already fetching multiple repos from this peer.
            return Err(TryFetchError::SessionCapacityReached);
        }

        let fetching = fetching.insert(FetchState {
            from,
            refs_at: refs_at.clone(),
            subscribers: vec![],
        });
        self.outbox.fetch(session, rid, refs_at, timeout);

        Ok(fetching)
    }

    pub fn fetched(
        &mut self,
        rid: RepoId,
        remote: NodeId,
        result: Result<fetch::FetchResult, FetchError>,
    ) {
        let Some(fetching) = self.fetching.remove(&rid) else {
            error!(target: "service", "Received unexpected fetch result for {rid}, from {remote}");
            return;
        };
        debug_assert_eq!(fetching.from, remote);

        if let Some(s) = self.sessions.get_mut(&remote) {
            // Mark this RID as fetched for this session.
            s.fetched(rid);
        }

        // Notify all fetch subscribers of the fetch result. This is used when the user requests
        // a fetch via the CLI, for example.
        for sub in &fetching.subscribers {
            debug!(target: "service", "Found existing fetch request from {remote}, sending result..");

            let result = match &result {
                Ok(success) => FetchResult::Success {
                    updated: success.updated.clone(),
                    namespaces: success.namespaces.clone(),
                    clone: success.clone,
                },
                Err(e) => FetchResult::Failed {
                    reason: e.to_string(),
                },
            };
            if sub.send(result).is_err() {
                error!(target: "service", "Error sending fetch result for {rid} from {remote}..");
            } else {
                debug!(target: "service", "Sent fetch result for {rid} from {remote}..");
            }
        }

        match result {
            Ok(fetch::FetchResult {
                updated,
                namespaces,
                clone,
                doc,
            }) => {
                info!(target: "service", "Fetched {rid} from {remote} successfully");
                // Update our routing table in case this fetch was user-initiated and doesn't
                // come from an announcement.
                self.seed_discovered(rid, remote, self.clock.into());

                for update in &updated {
                    if update.is_skipped() {
                        trace!(target: "service", "Ref skipped: {update} for {rid}");
                    } else {
                        debug!(target: "service", "Ref updated: {update} for {rid}");
                    }
                }
                self.emitter.emit(Event::RefsFetched {
                    remote,
                    rid,
                    updated: updated.clone(),
                });

                // Announce our new inventory if this fetch was a full clone.
                // Only update and announce inventory for public repositories.
                if clone && doc.visibility.is_public() {
                    debug!(target: "service", "Updating and announcing inventory for cloned repository {rid}..");

                    if let Err(e) = self.add_inventory(rid) {
                        error!(target: "service", "Error announcing inventory for {rid}: {e}");
                    }
                }

                // It's possible for a fetch to succeed but nothing was updated.
                if updated.is_empty() || updated.iter().all(|u| u.is_skipped()) {
                    debug!(target: "service", "Nothing to announce, no refs were updated..");
                } else {
                    // Finally, announce the refs. This is useful for nodes to know what we've synced,
                    // beyond just knowing that we have added an item to our inventory.
                    if let Err(e) = self.announce_refs(rid, doc.into(), namespaces) {
                        error!(target: "service", "Failed to announce new refs: {e}");
                    }
                }
            }
            Err(err) => {
                error!(target: "service", "Fetch failed for {rid} from {remote}: {err}");

                // For now, we only disconnect the remote in case of timeout. In the future,
                // there may be other reasons to disconnect.
                if err.is_timeout() {
                    self.outbox.disconnect(remote, DisconnectReason::Fetch(err));
                }
            }
        }
        // We can now try to dequeue more fetches.
        self.dequeue_fetches();
    }

    /// Attempt to dequeue fetches from all peers.
    /// At most one fetch is dequeued per peer. If the fetch cannot be processed,
    /// it is put back on the queue for that peer.
    ///
    /// Fetches are queued for two reasons:
    /// 1. The RID was already being fetched.
    /// 2. The session was already at fetch capacity.
    pub fn dequeue_fetches(&mut self) {
        let sessions = self
            .sessions
            .shuffled()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>();

        // Try to dequeue once per session.
        for nid in sessions {
            // SAFETY: All the keys we are iterating on exist.
            #[allow(clippy::unwrap_used)]
            let sess = self.sessions.get_mut(&nid).unwrap();
            if !sess.is_connected() || sess.is_at_capacity() {
                continue;
            }

            if let Some(QueuedFetch {
                rid,
                from,
                refs_at,
                timeout,
                channel,
            }) = sess.dequeue_fetch()
            {
                debug!(target: "service", "Dequeued fetch for {rid} from session {from}..");

                if let Some(refs) = NonEmpty::from_vec(refs_at) {
                    let repo_entry = self.policies.seed_policy(&rid).expect(
                        "Service::dequeue_fetch: error accessing repo seeding configuration",
                    );
                    let SeedingPolicy::Allow { scope } = repo_entry.policy else {
                        debug!(target: "service", "Repository {rid} is no longer seeded, skipping..");
                        continue;
                    };
                    self.fetch_refs_at(rid, from, refs, scope, timeout, channel);
                } else {
                    // If no refs are specified, always do a full fetch.
                    self.fetch(rid, from, timeout, channel);
                }
            }
        }
    }

    /// Inbound connection attempt.
    pub fn accepted(&mut self, ip: IpAddr) -> bool {
        // Always accept localhost connections, even if we already reached
        // our inbound connection limit.
        if ip.is_loopback() || ip.is_unspecified() {
            return true;
        }
        // Check for inbound connection limit.
        if self.sessions.inbound().count() >= self.config.limits.connection.inbound {
            return false;
        }
        match self.db.addresses().is_ip_banned(ip) {
            Ok(banned) => {
                if banned {
                    debug!(target: "service", "Rejecting inbound connection from banned ip {ip}");
                    return false;
                }
            }
            Err(e) => error!(target: "service", "Error querying ban status for {ip}: {e}"),
        }
        let host: HostName = ip.into();

        if self.limiter.limit(
            host.clone(),
            None,
            &self.config.limits.rate.inbound,
            self.clock,
        ) {
            trace!(target: "service", "Rate limiting inbound connection from {host}..");
            return false;
        }
        true
    }

    pub fn attempted(&mut self, nid: NodeId, addr: Address) {
        debug!(target: "service", "Attempted connection to {nid} ({addr})");

        if let Some(sess) = self.sessions.get_mut(&nid) {
            sess.to_attempted();
        } else {
            #[cfg(debug_assertions)]
            panic!("Service::attempted: unknown session {nid}@{addr}");
        }
    }

    pub fn listening(&mut self, local_addr: net::SocketAddr) {
        info!(target: "node", "Listening on {local_addr}..");

        self.listening.push(local_addr);
    }

    pub fn connected(&mut self, remote: NodeId, addr: Address, link: Link) {
        info!(target: "service", "Connected to {remote} ({addr}) ({link:?})");
        self.emitter.emit(Event::PeerConnected { nid: remote });

        let msgs = self.initial(link);

        if link.is_outbound() {
            if let Some(peer) = self.sessions.get_mut(&remote) {
                peer.to_connected(self.clock);
                self.outbox.write_all(peer, msgs);
            }
        } else {
            match self.sessions.entry(remote) {
                Entry::Occupied(mut e) => {
                    // In this scenario, it's possible that our peer is persistent, and
                    // disconnected. We get an inbound connection before we attempt a re-connection,
                    // and therefore we treat it as a regular inbound connection.
                    //
                    // It's also possible that a disconnection hasn't gone through yet and our
                    // peer is still in connected state here, while a new inbound connection from
                    // that same peer is made. This results in a new connection from a peer that is
                    // already connected from the perspective of the service. This appears to be
                    // a bug in the underlying networking library.
                    let peer = e.get_mut();
                    debug!(
                        target: "service",
                        "Connecting peer {remote} already has a session open ({peer})"
                    );
                    peer.link = link;
                    peer.to_connected(self.clock);
                    self.outbox.write_all(peer, msgs);
                }
                Entry::Vacant(e) => {
                    if let HostName::Ip(ip) = addr.host {
                        if !address::is_local(&ip) {
                            if let Err(e) =
                                self.db
                                    .addresses_mut()
                                    .record_ip(&remote, ip, self.clock.into())
                            {
                                log::error!(target: "service", "Error recording IP address for {remote}: {e}");
                            }
                        }
                    }
                    let peer = e.insert(Session::inbound(
                        remote,
                        addr,
                        self.config.is_persistent(&remote),
                        self.rng.clone(),
                        self.clock,
                        self.config.limits.clone(),
                    ));
                    self.outbox.write_all(peer, msgs);
                }
            }
        }
    }

    pub fn disconnected(&mut self, remote: NodeId, link: Link, reason: &DisconnectReason) {
        let since = self.local_time();
        let Some(session) = self.sessions.get_mut(&remote) else {
            // Since we sometimes disconnect the service eagerly, it's not unusual to get a second
            // disconnection event once the transport is dropped.
            trace!(target: "service", "Redundant disconnection for {} ({})", remote, reason);
            return;
        };
        // In cases of connection conflicts, there may be disconnections of one of the two
        // connections. In that case we don't want the service to remove the session.
        if session.link != link {
            return;
        }

        info!(target: "service", "Disconnected from {} ({})", remote, reason);
        self.emitter.emit(Event::PeerDisconnected {
            nid: remote,
            reason: reason.to_string(),
        });

        let link = session.link;
        let addr = session.addr.clone();

        self.fetching.retain(|_, fetching| {
            if fetching.from != remote {
                return true;
            }
            // Remove and fail any pending fetches from this remote node.
            for resp in &fetching.subscribers {
                resp.send(FetchResult::Failed {
                    reason: format!("disconnected: {reason}"),
                })
                .ok();
            }
            false
        });

        // Attempt to re-connect to persistent peers.
        if self.config.peer(&remote).is_some() {
            let delay = LocalDuration::from_secs(2u64.saturating_pow(session.attempts() as u32))
                .clamp(MIN_RECONNECTION_DELTA, MAX_RECONNECTION_DELTA);

            // Nb. We always try to reconnect to persistent peers, even when the error appears
            // to not be transient.
            session.to_disconnected(since, since + delay);

            debug!(target: "service", "Reconnecting to {remote} in {delay}..");

            self.outbox.wakeup(delay);
        } else {
            debug!(target: "service", "Dropping peer {remote}..");
            self.sessions.remove(&remote);

            let severity = match reason {
                DisconnectReason::Dial(_)
                | DisconnectReason::Fetch(_)
                | DisconnectReason::Connection(_) => {
                    if self.is_online() {
                        // If we're "online", there's something wrong with this
                        // peer connection specifically.
                        Severity::Medium
                    } else {
                        Severity::Low
                    }
                }
                DisconnectReason::Session(e) => e.severity(),
                DisconnectReason::Command
                | DisconnectReason::Conflict
                | DisconnectReason::SelfConnection => Severity::Low,
            };

            if let Err(e) = self
                .db
                .addresses_mut()
                .disconnected(&remote, &addr, severity)
            {
                error!(target: "service", "Error updating address store: {e}");
            }
            // Only re-attempt outbound connections, since we don't care if an inbound connection
            // is dropped.
            if link.is_outbound() {
                self.maintain_connections();
            }
        }
        self.dequeue_fetches();
    }

    pub fn received_message(&mut self, remote: NodeId, message: Message) {
        if let Err(err) = self.handle_message(&remote, message) {
            // If there's an error, stop processing messages from this peer.
            // However, we still relay messages returned up to this point.
            self.outbox
                .disconnect(remote, DisconnectReason::Session(err));

            // FIXME: The peer should be set in a state such that we don't
            // process further messages.
        }
    }

    /// Handle an announcement message.
    ///
    /// Returns `true` if this announcement should be stored and relayed to connected peers,
    /// and `false` if it should not.
    pub fn handle_announcement(
        &mut self,
        relayer: &NodeId,
        relayer_addr: &Address,
        announcement: &Announcement,
    ) -> Result<Option<gossip::AnnouncementId>, session::Error> {
        if !announcement.verify() {
            return Err(session::Error::Misbehavior);
        }
        let Announcement {
            node: announcer,
            message,
            ..
        } = announcement;

        // Ignore our own announcements, in case the relayer sent one by mistake.
        if announcer == self.nid() {
            return Ok(None);
        }
        let now = self.clock;
        let timestamp = message.timestamp();

        // Don't allow messages from too far in the future.
        if timestamp.saturating_sub(now.as_millis()) > MAX_TIME_DELTA.as_millis() as u64 {
            return Err(session::Error::InvalidTimestamp(timestamp));
        }

        // We don't process announcements from nodes we don't know, since the node announcement is
        // what provides DoS protection.
        //
        // Note that it's possible to *not* receive the node announcement, but receive the
        // subsequent announcements of a node in the case of historical gossip messages requested
        // from the `subscribe` message. This can happen if the cut-off time is after the node
        // announcement timestamp, but before the other announcements. In that case, we simply
        // ignore all announcements of that node until we get a node announcement.
        if let AnnouncementMessage::Inventory(_) | AnnouncementMessage::Refs(_) = message {
            match self.db.addresses().get(announcer) {
                Ok(node) => {
                    if node.is_none() {
                        debug!(target: "service", "Ignoring announcement from unknown node {announcer} (t={timestamp})");
                        return Ok(None);
                    }
                }
                Err(e) => {
                    error!(target: "service", "Error looking up node in address book: {e}");
                    return Ok(None);
                }
            }
        }

        // Discard announcement messages we've already seen, otherwise update our last seen time.
        let relay = match self.db.gossip_mut().announced(announcer, announcement) {
            Ok(Some(id)) => {
                log::debug!(
                    target: "service",
                    "Stored announcement from {announcer} to be broadcast in {} (t={timestamp})",
                    (self.last_gossip + GOSSIP_INTERVAL) - self.clock
                );
                // Keep track of who relayed the message for later.
                self.relayed_by.entry(id).or_default().push(*relayer);

                // Decide whether or not to relay this message, if it's fresh.
                // To avoid spamming peers on startup with historical gossip messages,
                // don't relay messages that are too old. We make an exception for node announcements,
                // since they are cached, and will hence often carry old timestamps.
                let relay = message.is_node_announcement()
                    || now - timestamp.to_local_time() <= MAX_TIME_DELTA;
                relay.then_some(id)
            }
            Ok(None) => {
                // FIXME: Still mark as relayed by this peer.
                // FIXME: Refs announcements should not be delayed, since they are only sent
                // to subscribers.
                debug!(target: "service", "Ignoring stale announcement from {announcer} (t={timestamp})");
                return Ok(None);
            }
            Err(e) => {
                error!(target: "service", "Error updating gossip entry from {announcer}: {e}");
                return Ok(None);
            }
        };

        match message {
            // Process a peer inventory update announcement by (maybe) fetching.
            AnnouncementMessage::Inventory(message) => {
                self.emitter.emit(Event::InventoryAnnounced {
                    nid: *announcer,
                    inventory: message.inventory.to_vec(),
                    timestamp: message.timestamp,
                });
                match self.sync_routing(
                    message.inventory.iter().cloned(),
                    *announcer,
                    message.timestamp,
                ) {
                    Ok(synced) => {
                        if synced.is_empty() {
                            trace!(target: "service", "No routes updated by inventory announcement from {announcer}");
                            return Ok(None);
                        }
                    }
                    Err(e) => {
                        error!(target: "service", "Error processing inventory from {announcer}: {e}");
                        return Ok(None);
                    }
                }
                let mut missing = Vec::new();
                let nid = *self.nid();

                // Here we handle the special case where the inventory we received is that of
                // a connected peer, as opposed to being relayed to us.
                if let Some(sess) = self.sessions.get_mut(announcer) {
                    for id in message.inventory.as_slice() {
                        // If we are connected to the announcer of this inventory, update the peer's
                        // subscription filter to include all inventory items. This way, we'll
                        // relay messages relating to the peer's inventory.
                        if let Some(sub) = &mut sess.subscribe {
                            sub.filter.insert(id);
                        }

                        // If we're seeding and connected to the announcer, and we don't have
                        // the inventory, fetch it from the announcer.
                        if self.policies.is_seeding(id).expect(
                            "Service::handle_announcement: error accessing seeding configuration",
                        ) {
                            // Only if we do not have the repository locally do we fetch here.
                            // If we do have it, only fetch after receiving a ref announcement.
                            match self.db.routing().entry(id, &nid) {
                                Ok(entry) => {
                                    if entry.is_none() {
                                        missing.push(*id);
                                    }
                                }
                                Err(e) => error!(
                                    target: "service",
                                    "Error checking local inventory for {id}: {e}"
                                ),
                            }
                        }
                    }
                }
                // Since we have limited fetch capacity, it may be that we can't fetch an entire
                // inventory from a peer. Therefore we randomize the order of the RIDs to fetch
                // different RIDs from different peers in case multiple peers announce the same
                // RIDs.
                self.rng.shuffle(&mut missing);

                for rid in missing {
                    debug!(target: "service", "Missing seeded inventory {rid}; initiating fetch..");
                    self.fetch(rid, *announcer, FETCH_TIMEOUT, None);
                }
                return Ok(relay);
            }
            AnnouncementMessage::Refs(message) => {
                self.emitter.emit(Event::RefsAnnounced {
                    nid: *announcer,
                    rid: message.rid,
                    refs: message.refs.to_vec(),
                    timestamp: message.timestamp,
                });
                // Empty announcements can be safely ignored.
                let Some(refs) = NonEmpty::from_vec(message.refs.to_vec()) else {
                    debug!(target: "service", "Skipping fetch, no refs in announcement for {} (t={timestamp})", message.rid);
                    return Ok(None);
                };
                // We update inventories when receiving ref announcements, as these could come
                // from a new repository being initialized.
                self.seed_discovered(message.rid, *announcer, message.timestamp);

                // Update sync status of announcer for this repo.
                if let Some(refs) = refs.iter().find(|r| &r.remote == self.nid()) {
                    debug!(
                        target: "service",
                        "Refs announcement of {announcer} for {} contains our own remote at {} (t={})",
                        message.rid, refs.at, message.timestamp
                    );
                    match self.db.seeds_mut().synced(
                        &message.rid,
                        announcer,
                        refs.at,
                        message.timestamp,
                    ) {
                        Ok(updated) => {
                            if updated {
                                debug!(
                                    target: "service",
                                    "Updating sync status of {announcer} for {} to {}",
                                    message.rid, refs.at
                                );
                                self.emitter.emit(Event::RefsSynced {
                                    rid: message.rid,
                                    remote: *announcer,
                                    at: refs.at,
                                });
                            } else {
                                debug!(
                                    target: "service",
                                    "Sync status of {announcer} was not updated for {}",
                                    message.rid,
                                );
                            }
                        }
                        Err(e) => {
                            error!(target: "service", "Error updating sync status for {}: {e}", message.rid);
                        }
                    }
                }
                let repo_entry = self.policies.seed_policy(&message.rid).expect(
                    "Service::handle_announcement: error accessing repo seeding configuration",
                );
                let SeedingPolicy::Allow { scope } = repo_entry.policy else {
                    debug!(
                        target: "service",
                        "Ignoring refs announcement from {announcer}: repository {} isn't seeded (t={timestamp})",
                        message.rid
                    );
                    return Ok(None);
                };
                // Refs can be relayed by peers who don't have the data in storage,
                // therefore we only check whether we are connected to the *announcer*,
                // which is required by the protocol to only announce refs it has.
                let Some(remote) = self.sessions.get(announcer).cloned() else {
                    trace!(
                        target: "service",
                        "Skipping fetch of {}, no sessions connected to {announcer}",
                        message.rid
                    );
                    return Ok(relay);
                };
                // Finally, start the fetch.
                self.fetch_refs_at(message.rid, remote.id, refs, scope, FETCH_TIMEOUT, None);

                return Ok(relay);
            }
            AnnouncementMessage::Node(
                ann @ NodeAnnouncement {
                    features,
                    addresses,
                    ..
                },
            ) => {
                self.emitter.emit(Event::NodeAnnounced {
                    nid: *announcer,
                    alias: ann.alias.clone(),
                    timestamp: ann.timestamp,
                    features: *features,
                    addresses: addresses.to_vec(),
                });
                // If this node isn't a seed, we're not interested in adding it
                // to our address book, but other nodes may be, so we relay the message anyway.
                if !features.has(Features::SEED) {
                    return Ok(relay);
                }

                match self.db.addresses_mut().insert(
                    announcer,
                    ann.version,
                    ann.features,
                    &ann.alias,
                    ann.work(),
                    &ann.agent,
                    timestamp,
                    addresses
                        .iter()
                        // Ignore non-routable addresses unless received from a local network
                        // peer. This allows the node to function in a local network.
                        .filter(|a| a.is_routable() || relayer_addr.is_local())
                        .map(|a| KnownAddress::new(a.clone(), address::Source::Peer)),
                ) {
                    Ok(updated) => {
                        // Only relay if we received new information.
                        if updated {
                            debug!(
                                target: "service",
                                "Address store entry for node {announcer} updated at {timestamp}"
                            );
                            return Ok(relay);
                        }
                    }
                    Err(err) => {
                        // An error here is due to a fault in our address store.
                        error!(target: "service", "Error processing node announcement from {announcer}: {err}");
                    }
                }
            }
        }
        Ok(None)
    }

    pub fn handle_info(&mut self, remote: NodeId, info: &Info) -> Result<(), session::Error> {
        match info {
            // Nb. We don't currently send this message.
            Info::RefsAlreadySynced { rid, at } => {
                debug!(target: "service", "Refs already synced for {rid} by {remote}");
                self.emitter.emit(Event::RefsSynced {
                    rid: *rid,
                    remote,
                    at: *at,
                });
            }
        }

        Ok(())
    }

    pub fn handle_message(
        &mut self,
        remote: &NodeId,
        message: Message,
    ) -> Result<(), session::Error> {
        let local = self.node_id();
        let relay = self.config.is_relay();
        let Some(peer) = self.sessions.get_mut(remote) else {
            warn!(target: "service", "Session not found for {remote}");
            return Ok(());
        };
        peer.last_active = self.clock;

        let limit = match peer.link {
            Link::Outbound => &self.config.limits.rate.outbound,
            Link::Inbound => &self.config.limits.rate.inbound,
        };
        if self
            .limiter
            .limit(peer.addr.clone().into(), Some(remote), limit, self.clock)
        {
            debug!(target: "service", "Rate limiting message from {remote} ({})", peer.addr);
            return Ok(());
        }
        message.log(log::Level::Debug, remote, Link::Inbound);

        let connected = match &mut peer.state {
            session::State::Disconnected { .. } => {
                debug!(target: "service", "Ignoring message from disconnected peer {}", peer.id);
                return Ok(());
            }
            // In case of a discrepancy between the service state and the state of the underlying
            // wire protocol, we may receive a message from a peer that we consider not fully connected
            // at the service level. To remedy this, we simply transition the peer to a connected state.
            //
            // This is not ideal, but until the wire protocol and service are unified, it's the simplest
            // solution to converge towards the same state.
            session::State::Attempted { .. } | session::State::Initial => {
                debug!(target: "service", "Received unexpected message from connecting peer {}", peer.id);
                debug!(target: "service", "Transitioning peer {} to 'connected' state", peer.id);

                peer.to_connected(self.clock);

                None
            }
            session::State::Connected {
                ping, latencies, ..
            } => Some((ping, latencies)),
        };

        trace!(target: "service", "Received message {message:?} from {remote}");

        match message {
            // Process a peer announcement.
            Message::Announcement(ann) => {
                let relayer = remote;
                let relayer_addr = peer.addr.clone();

                if let Some(id) = self.handle_announcement(relayer, &relayer_addr, &ann)? {
                    if self.config.is_relay() {
                        if let AnnouncementMessage::Inventory(_) = ann.message {
                            if let Err(e) = self
                                .database_mut()
                                .gossip_mut()
                                .set_relay(id, gossip::RelayStatus::Relay)
                            {
                                error!(target: "service", "Error setting relay flag for message: {e}");
                                return Ok(());
                            }
                        } else {
                            self.relay(id, ann);
                        }
                    }
                }
            }
            Message::Subscribe(subscribe) => {
                // Filter announcements by interest.
                match self
                    .db
                    .gossip()
                    .filtered(&subscribe.filter, subscribe.since, subscribe.until)
                {
                    Ok(anns) => {
                        for ann in anns {
                            let ann = match ann {
                                Ok(a) => a,
                                Err(e) => {
                                    error!(target: "service", "Error reading gossip message from store: {e}");
                                    continue;
                                }
                            };
                            // Don't send announcements authored by the remote, back to the remote.
                            if ann.node == *remote {
                                continue;
                            }
                            // Only send messages if we're a relay, or it's our own messages.
                            if relay || ann.node == local {
                                self.outbox.write(peer, ann.into());
                            }
                        }
                    }
                    Err(e) => {
                        error!(target: "service", "Error querying gossip messages from store: {e}");
                    }
                }
                peer.subscribe = Some(subscribe);
            }
            Message::Info(info) => {
                self.handle_info(*remote, &info)?;
            }
            Message::Ping(Ping { ponglen, .. }) => {
                // Ignore pings which ask for too much data.
                if ponglen > Ping::MAX_PONG_ZEROES {
                    return Ok(());
                }
                self.outbox.write(
                    peer,
                    Message::Pong {
                        zeroes: ZeroBytes::new(ponglen),
                    },
                );
            }
            Message::Pong { zeroes } => {
                if let Some((ping, latencies)) = connected {
                    if let session::PingState::AwaitingResponse {
                        len: ponglen,
                        since,
                    } = *ping
                    {
                        if (ponglen as usize) == zeroes.len() {
                            *ping = session::PingState::Ok;
                            // Keep track of peer latency.
                            latencies.push_back(self.clock - since);
                            if latencies.len() > MAX_LATENCIES {
                                latencies.pop_front();
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// A convenient method to check if we should fetch from a `RefsAnnouncement` with `scope`.
    fn refs_status_of(
        &self,
        rid: RepoId,
        refs: NonEmpty<RefsAt>,
        scope: &policy::Scope,
    ) -> Result<RefsStatus, Error> {
        let mut refs = RefsStatus::new(rid, refs, self.db.refs())?;
        // Check that there's something we want.
        if refs.want.is_empty() {
            return Ok(refs);
        }
        // Check scope.
        let mut refs = match scope {
            policy::Scope::All => refs,
            policy::Scope::Followed => match self.policies.namespaces_for(&self.storage, &rid) {
                Ok(Namespaces::All) => refs,
                Ok(Namespaces::Followed(followed)) => {
                    refs.want.retain(|r| followed.contains(&r.remote));
                    refs
                }
                Err(e) => return Err(e.into()),
            },
        };
        // Remove our own remote, we don't want to fetch that.
        refs.want.retain(|r| r.remote != self.node_id());

        Ok(refs)
    }

    /// Add a seed to our routing table.
    fn seed_discovered(&mut self, rid: RepoId, nid: NodeId, time: Timestamp) {
        if let Ok(result) = self.db.routing_mut().add_inventory([&rid], nid, time) {
            if let &[(_, InsertResult::SeedAdded)] = result.as_slice() {
                self.emitter.emit(Event::SeedDiscovered { rid, nid });
                info!(target: "service", "Routing table updated for {} with seed {nid}", rid);
            }
        }
    }

    /// Set of initial messages to send to a peer.
    fn initial(&mut self, _link: Link) -> Vec<Message> {
        let now = self.clock();
        let filter = self.filter();

        // TODO: Only subscribe to outbound connections, otherwise we will consume too
        // much bandwidth.

        // If we've been previously connected to the network, we'll have received gossip messages.
        // Instead of simply taking the last timestamp we try to ensure we don't miss any
        // messages due un-synchronized clocks.
        //
        // If this is our first connection to the network, we just ask for a fixed backlog
        // of messages to get us started.
        let since = if let Some(last) = self.last_online_at {
            Timestamp::from(last - SUBSCRIBE_BACKLOG_DELTA)
        } else {
            (*now - INITIAL_SUBSCRIBE_BACKLOG_DELTA).into()
        };
        debug!(target: "service", "Subscribing to messages since timestamp {since}..");

        vec![
            Message::node(self.node.clone(), &self.signer),
            Message::inventory(self.inventory.clone(), &self.signer),
            Message::subscribe(filter, since, Timestamp::MAX),
        ]
    }

    /// Try to guess whether we're online or not.
    fn is_online(&self) -> bool {
        self.sessions
            .connected()
            .filter(|(_, s)| s.addr.is_routable() && s.last_active >= self.clock - IDLE_INTERVAL)
            .count()
            > 0
    }

    /// Remove a local repository from our inventory.
    fn remove_inventory(&mut self, rid: &RepoId) -> Result<bool, Error> {
        let node = self.node_id();
        let now = self.timestamp();

        let removed = self.db.routing_mut().remove_inventory(rid, &node)?;
        if removed {
            self.refresh_and_announce_inventory(now)?;
        }
        Ok(removed)
    }

    /// Add a local repository to our inventory.
    fn add_inventory(&mut self, rid: RepoId) -> Result<bool, Error> {
        let node = self.node_id();
        let now = self.timestamp();

        if !self.storage.contains(&rid)? {
            error!(target: "service", "Attempt to add non-existing inventory {rid}: repository not found in storage");
            return Ok(false);
        }
        // Add to our local inventory.
        let updates = self.db.routing_mut().add_inventory([&rid], node, now)?;
        let updated = !updates.is_empty();

        if updated {
            self.refresh_and_announce_inventory(now)?;
        }
        Ok(updated)
    }

    /// Update cached inventory message, and announce new inventory to peers.
    fn refresh_and_announce_inventory(&mut self, time: Timestamp) -> Result<(), Error> {
        let inventory = self.inventory()?;

        self.inventory = gossip::inventory(time, inventory);
        self.announce_inventory();

        Ok(())
    }

    /// Get our local inventory.
    ///
    /// A node's inventory is the advertized list of repositories offered by a node.
    ///
    /// A node's inventory consists of *public* repositories that are seeded and available locally
    /// in the node's storage. We use the routing table as the canonical state of all inventories,
    /// including the local node's.
    ///
    /// When a repository is unseeded, it is also removed from the inventory. Private repositories
    /// are *not* part of a node's inventory.
    fn inventory(&self) -> Result<HashSet<RepoId>, Error> {
        self.db
            .routing()
            .get_inventory(self.nid())
            .map_err(Error::from)
    }

    /// Process a peer inventory announcement by updating our routing table.
    /// This function expects the peer's full inventory, and prunes entries that are not in the
    /// given inventory.
    fn sync_routing(
        &mut self,
        inventory: impl IntoIterator<Item = RepoId>,
        from: NodeId,
        timestamp: Timestamp,
    ) -> Result<SyncedRouting, Error> {
        let mut synced = SyncedRouting::default();
        let included = inventory.into_iter().collect::<BTreeSet<_>>();

        for (rid, result) in
            self.db
                .routing_mut()
                .add_inventory(included.iter(), from, timestamp)?
        {
            match result {
                InsertResult::SeedAdded => {
                    info!(target: "service", "Routing table updated for {rid} with seed {from}");
                    self.emitter.emit(Event::SeedDiscovered { rid, nid: from });

                    if self
                        .policies
                        .is_seeding(&rid)
                        .expect("Service::process_inventory: error accessing seeding configuration")
                    {
                        // TODO: We should fetch here if we're already connected, case this seed has
                        // refs we don't have.
                    }
                    synced.added.push(rid);
                }
                InsertResult::TimeUpdated => {
                    synced.updated.push(rid);
                }
                InsertResult::NotUpdated => {}
            }
        }
        for rid in self.db.routing().get_inventory(&from)?.into_iter() {
            if !included.contains(&rid) {
                if self.db.routing_mut().remove_inventory(&rid, &from)? {
                    synced.removed.push(rid);
                    self.emitter.emit(Event::SeedDropped { rid, nid: from });
                }
            }
        }
        Ok(synced)
    }

    /// Return a refs announcement including the given remotes.
    fn refs_announcement_for(
        &mut self,
        rid: RepoId,
        remotes: impl IntoIterator<Item = NodeId>,
    ) -> Result<(Announcement, Vec<RefsAt>), Error> {
        let repo = self.storage.repository(rid)?;
        let timestamp = self.timestamp();
        let mut refs = BoundedVec::<_, REF_REMOTE_LIMIT>::new();

        for remote_id in remotes.into_iter() {
            let refs_at = RefsAt::new(&repo, remote_id)?;

            if refs.push(refs_at).is_err() {
                warn!(
                    target: "service",
                    "refs announcement limit ({}) exceeded, peers will see only some of your repository references",
                    REF_REMOTE_LIMIT,
                );
                break;
            }
        }

        let msg = AnnouncementMessage::from(RefsAnnouncement {
            rid,
            refs: refs.clone(),
            timestamp,
        });
        Ok((msg.signed(&self.signer), refs.into()))
    }

    /// Announce our own refs for the given repo.
    fn announce_own_refs(&mut self, rid: RepoId, doc: Doc<Verified>) -> Result<Vec<RefsAt>, Error> {
        let (refs, timestamp) = self.announce_refs(rid, doc, [self.node_id()])?;

        // Update refs database with our signed refs branches.
        // This isn't strictly necessary for now, as we only use the database for fetches, and
        // we don't fetch our own refs that are announced, but it's for good measure.
        if let &[r] = refs.as_slice() {
            self.emitter.emit(Event::LocalRefsAnnounced {
                rid,
                refs: r,
                timestamp,
            });
            if let Err(e) = self.database_mut().refs_mut().set(
                &rid,
                &r.remote,
                &SIGREFS_BRANCH,
                r.at,
                timestamp.to_local_time(),
            ) {
                error!(
                    target: "service",
                    "Error updating refs database for `rad/sigrefs` of {} in {rid}: {e}",
                    r.remote
                );
            }
        }
        Ok(refs)
    }

    /// Announce local refs for given repo.
    fn announce_refs(
        &mut self,
        rid: RepoId,
        doc: Doc<Verified>,
        remotes: impl IntoIterator<Item = NodeId>,
    ) -> Result<(Vec<RefsAt>, Timestamp), Error> {
        let (ann, refs) = self.refs_announcement_for(rid, remotes)?;
        let timestamp = ann.timestamp();
        let peers = self.sessions.connected().map(|(_, p)| p);

        // Update our sync status for our own refs. This is useful for determining if refs were
        // updated while the node was stopped.
        if let Some(refs) = refs.iter().find(|r| r.remote == ann.node) {
            info!(
                target: "service",
                "Announcing own refs for {rid} to peers ({}) (t={timestamp})..",
                refs.at
            );
            // Update our local node's sync status to mark the refs as announced.
            if let Err(e) = self
                .db
                .seeds_mut()
                .synced(&rid, &ann.node, refs.at, timestamp)
            {
                error!(target: "service", "Error updating sync status for local node: {e}");
            } else {
                debug!(target: "service", "Saved local sync status for {rid}..");
            }
        }

        self.outbox.announce(
            ann,
            peers.filter(|p| {
                // Only announce to peers who are allowed to view this repo.
                doc.is_visible_to(&p.id)
            }),
            self.db.gossip_mut(),
        );
        Ok((refs, timestamp))
    }

    fn reconnect(&mut self, nid: NodeId, addr: Address) -> bool {
        if let Some(sess) = self.sessions.get_mut(&nid) {
            sess.to_initial();
            self.outbox.connect(nid, addr);

            return true;
        }
        false
    }

    fn connect(&mut self, nid: NodeId, addr: Address) -> bool {
        debug!(target: "service", "Connecting to {nid} ({addr})..");

        if self.sessions.contains_key(&nid) {
            warn!(target: "service", "Attempted connection to peer {nid} which already has a session");
            return false;
        }
        if nid == self.node_id() {
            error!(target: "service", "Attempted connection to self");
            return false;
        }
        if self.sessions.outbound().count() >= self.config.limits.connection.outbound {
            error!(target: "service", "Outbound connection limit reached when attempting {nid} ({addr})");
            return false;
        }
        let persistent = self.config.is_persistent(&nid);
        let timestamp: Timestamp = self.clock.into();

        if let Err(e) = self.db.addresses_mut().attempted(&nid, &addr, timestamp) {
            error!(target: "service", "Error updating address book with connection attempt: {e}");
        }
        self.sessions.insert(
            nid,
            Session::outbound(
                nid,
                addr.clone(),
                persistent,
                self.rng.clone(),
                self.config.limits.clone(),
            ),
        );
        self.outbox.connect(nid, addr);

        true
    }

    fn seeds(&self, rid: &RepoId) -> Result<Seeds, Error> {
        let mut seeds = Seeds::new(self.rng.clone());

        // First build a list from peers that have synced our own refs, if any.
        // This step is skipped if we don't have the repository yet, or don't have
        // our own refs.
        if let Ok(repo) = self.storage.repository(*rid) {
            if let Ok(local) = RefsAt::new(&repo, self.node_id()) {
                for seed in self.db.seeds().seeds_for(rid)? {
                    let seed = seed?;
                    let state = self.sessions.get(&seed.nid).map(|s| s.state.clone());
                    let synced = if local.at == seed.synced_at.oid {
                        SyncStatus::Synced { at: seed.synced_at }
                    } else {
                        let local = SyncedAt::new(local.at, &repo)?;

                        SyncStatus::OutOfSync {
                            local,
                            remote: seed.synced_at,
                        }
                    };
                    seeds.insert(Seed::new(seed.nid, seed.addresses, state, Some(synced)));
                }
            }
        }

        // Then, add peers we know about but have no information about the sync status.
        // These peers have announced that they seed the repository via an inventory
        // announcement, but we haven't received any ref announcements from them.
        for nid in self.db.routing().get(rid)? {
            if nid == self.node_id() {
                continue;
            }
            if seeds.contains(&nid) {
                // We already have a richer entry for this node.
                continue;
            }
            let addrs = self.db.addresses().addresses_of(&nid)?;
            let state = self.sessions.get(&nid).map(|s| s.state.clone());

            seeds.insert(Seed::new(nid, addrs, state, None));
        }
        Ok(seeds)
    }

    /// Return a new filter object, based on our seeding policy.
    fn filter(&self) -> Filter {
        if self.config.seeding_policy.is_allow() {
            // TODO: Remove bits for blocked repos.
            Filter::default()
        } else {
            self.filter.clone()
        }
    }

    /// Get a timestamp for using in announcements.
    /// Never returns the same timestamp twice.
    fn timestamp(&mut self) -> Timestamp {
        let now = Timestamp::from(self.clock);
        if *now > *self.last_timestamp {
            self.last_timestamp = now;
        } else {
            self.last_timestamp = self.last_timestamp + 1;
        }
        self.last_timestamp
    }

    fn relay(&mut self, id: gossip::AnnouncementId, ann: Announcement) {
        let announcer = ann.node;
        let relayed_by = self.relayed_by.get(&id);
        let rid = if let AnnouncementMessage::Refs(RefsAnnouncement { rid, .. }) = ann.message {
            Some(rid)
        } else {
            None
        };
        // Choose peers we should relay this message to.
        // 1. Don't relay to a peer who sent us this message.
        // 2. Don't relay to the peer who signed this announcement.
        let relay_to = self
            .sessions
            .connected()
            .filter(|(id, _)| {
                relayed_by
                    .map(|relayers| !relayers.contains(id))
                    .unwrap_or(true) // If there are no relayers we let it through.
            })
            .filter(|(id, _)| **id != announcer)
            .filter(|(id, _)| {
                if let Some(rid) = rid {
                    // Only relay this message if the peer is allowed to know about the
                    // repository. If we don't have the repository, return `false` because
                    // we can't determine if it's private or public.
                    self.storage
                        .get(rid)
                        .ok()
                        .flatten()
                        .map(|doc| doc.is_visible_to(id))
                        .unwrap_or(false)
                } else {
                    // Announcement doesn't concern a specific repository, let it through.
                    true
                }
            })
            .map(|(_, p)| p);

        self.outbox.relay(ann, relay_to);
    }

    ////////////////////////////////////////////////////////////////////////////
    // Periodic tasks
    ////////////////////////////////////////////////////////////////////////////

    fn relay_announcements(&mut self) -> Result<(), Error> {
        let now = self.clock.into();
        let rows = self.database_mut().gossip_mut().relays(now)?;
        let local = self.node_id();

        for (id, msg) in rows {
            let announcer = msg.node;
            if announcer == local {
                // Don't relay our own stored gossip messages.
                continue;
            }
            self.relay(id, msg);
        }
        Ok(())
    }

    /// Announce our inventory to all connected peers, unless it was already announced.
    fn announce_inventory(&mut self) {
        let timestamp = self.inventory.timestamp.to_local_time();

        if self.last_inventory == timestamp {
            debug!(target: "service", "Skipping redundant inventory announcement (t={})", self.inventory.timestamp);
            return;
        }
        let msg = AnnouncementMessage::from(self.inventory.clone());

        self.outbox.announce(
            msg.signed(&self.signer),
            self.sessions.connected().map(|(_, p)| p),
            self.db.gossip_mut(),
        );
        self.last_inventory = timestamp;
    }

    fn prune_routing_entries(&mut self, now: &LocalTime) -> Result<(), routing::Error> {
        let count = self.db.routing().len()?;
        if count <= self.config.limits.routing_max_size {
            return Ok(());
        }

        let delta = count - self.config.limits.routing_max_size;
        let nid = self.node_id();
        self.db.routing_mut().prune(
            (*now - self.config.limits.routing_max_age).into(),
            Some(delta),
            &nid,
        )?;
        Ok(())
    }

    fn disconnect_unresponsive_peers(&mut self, now: &LocalTime) {
        let stale = self
            .sessions
            .connected()
            .filter(|(_, session)| *now - session.last_active >= STALE_CONNECTION_TIMEOUT);

        for (_, session) in stale {
            debug!(target: "service", "Disconnecting unresponsive peer {}..", session.id);

            // TODO: Should we switch the session state to "disconnected" even before receiving
            // an official "disconnect"? Otherwise we keep pinging until we get the disconnection.

            self.outbox.disconnect(
                session.id,
                DisconnectReason::Session(session::Error::Timeout),
            );
        }
    }

    /// Ensure connection health by pinging connected peers.
    fn keep_alive(&mut self, now: &LocalTime) {
        let inactive_sessions = self
            .sessions
            .connected_mut()
            .filter(|(_, session)| *now - session.last_active >= KEEP_ALIVE_DELTA)
            .map(|(_, session)| session);
        for session in inactive_sessions {
            session.ping(self.clock, &mut self.outbox).ok();
        }
    }

    /// Get a list of peers available to connect to, sorted by lowest penalty.
    fn available_peers(&mut self) -> Vec<Peer> {
        match self.db.addresses().entries() {
            Ok(entries) => {
                // Nb. we don't want to connect to any peers that already have a session with us,
                // even if it's in a disconnected state. Those sessions are re-attempted automatically.
                let mut peers = entries
                    .filter(|entry| entry.version == PROTOCOL_VERSION)
                    .filter(|entry| !entry.address.banned)
                    .filter(|entry| !entry.penalty.is_connect_threshold_reached())
                    .filter(|entry| !self.sessions.contains_key(&entry.node))
                    .filter(|entry| !self.config.external_addresses.contains(&entry.address.addr))
                    .filter(|entry| &entry.node != self.nid())
                    .fold(HashMap::new(), |mut acc, entry| {
                        acc.entry(entry.node)
                            .and_modify(|e: &mut Peer| e.addresses.push(entry.address.clone()))
                            .or_insert_with(|| Peer {
                                nid: entry.node,
                                addresses: vec![entry.address],
                                penalty: entry.penalty,
                            });
                        acc
                    })
                    .into_values()
                    .collect::<Vec<_>>();
                peers.sort_by_key(|p| p.penalty);
                peers
            }
            Err(e) => {
                error!(target: "service", "Unable to lookup available peers in address book: {e}");
                Vec::new()
            }
        }
    }

    /// Fetch all repositories that are seeded but missing from storage.
    fn fetch_missing_repositories(&mut self) -> Result<(), Error> {
        for policy in self.policies.seed_policies()? {
            let rid = policy.rid;

            if !policy.is_allow() {
                continue;
            }
            if self.storage.contains(&rid)? {
                continue;
            }
            match self.seeds(&rid) {
                Ok(seeds) => {
                    if let Some(connected) = NonEmpty::from_vec(seeds.connected().collect()) {
                        for seed in connected {
                            self.fetch(rid, seed.nid, FETCH_TIMEOUT, None);
                        }
                    } else {
                        // TODO: We should make sure that this fetch is retried later, either
                        // when we connect to a seed, or when we discover a new seed.
                        // Since new connections and routing table updates are both conditions for
                        // fetching, we should trigger fetches when those conditions appear.
                        // Another way to handle this would be to update our database, saying
                        // that we're trying to fetch a certain repo. We would then just
                        // iterate over those entries in the above circumstances. This is
                        // merely an optimization though, we can also iterate over all seeded
                        // repos and check which ones are not in our inventory.
                        debug!(target: "service", "No connected seeds found for {rid}..");
                    }
                }
                Err(e) => {
                    error!(target: "service", "Couldn't fetch missing repo {rid}: failed to lookup seeds: {e}");
                }
            }
        }
        Ok(())
    }

    /// Run idle task for all connections.
    fn idle_connections(&mut self) {
        for (_, sess) in self.sessions.iter_mut() {
            sess.idle(self.clock);

            if sess.is_stable() {
                // Mark as connected once connection is stable.
                if let Err(e) =
                    self.db
                        .addresses_mut()
                        .connected(&sess.id, &sess.addr, self.clock.into())
                {
                    error!(target: "service", "Error updating address book with connection: {e}");
                }
            }
        }
    }

    /// Try to maintain a target number of connections.
    fn maintain_connections(&mut self) {
        let PeerConfig::Dynamic = self.config.peers else {
            return;
        };
        trace!(target: "service", "Maintaining connections..");

        let target = TARGET_OUTBOUND_PEERS;
        let now = self.clock;
        let outbound = self
            .sessions
            .values()
            .filter(|s| s.link.is_outbound())
            .filter(|s| s.is_connected() || s.is_connecting())
            .count();
        let wanted = target.saturating_sub(outbound);

        // Don't connect to more peers than needed.
        if wanted == 0 {
            return;
        }

        // Peers available to connect to.
        let available = self
            .available_peers()
            .into_iter()
            .filter_map(|peer| {
                peer.addresses
                    .into_iter()
                    .find(|ka| match (ka.last_success, ka.last_attempt) {
                        // If we succeeded the last time we tried, this is a good address.
                        // If it's been long enough that we failed to connect, we also try again.
                        (Some(success), Some(attempt)) => {
                            success >= attempt || now - attempt >= CONNECTION_RETRY_DELTA
                        }
                        // If we haven't succeeded yet, and we waited long enough, we can try this address.
                        (None, Some(attempt)) => now - attempt >= CONNECTION_RETRY_DELTA,
                        // If we have no failed attempts for this address, it's worth a try.
                        (_, None) => true,
                    })
                    .map(|ka| (peer.nid, ka))
            })
            .filter(|(_, ka)| match AddressType::from(&ka.addr) {
                // Only consider onion addresses if configured.
                AddressType::Onion => self.config.onion.is_some(),
                AddressType::Dns | AddressType::Ipv4 | AddressType::Ipv6 => true,
            });

        // Peers we are going to attempt connections to.
        let connect = available.take(wanted).collect::<Vec<_>>();
        if connect.len() < wanted {
            log::debug!(
                target: "service",
                "Not enough available peers to connect to (available={}, wanted={wanted})",
                connect.len()
            );
        }
        for (id, ka) in connect {
            self.connect(id, ka.addr.clone());
        }
    }

    /// Maintain persistent peer connections.
    fn maintain_persistent(&mut self) {
        trace!(target: "service", "Maintaining persistent peers..");

        let now = self.local_time();
        let mut reconnect = Vec::new();

        for (nid, session) in self.sessions.iter_mut() {
            if let Some(addr) = self.config.peer(nid) {
                if let session::State::Disconnected { retry_at, .. } = &mut session.state {
                    // TODO: Try to reconnect only if the peer was attempted. A disconnect without
                    // even a successful attempt means that we're unlikely to be able to reconnect.

                    if now >= *retry_at {
                        reconnect.push((*nid, addr.clone(), session.attempts()));
                    }
                }
            }
        }

        for (nid, addr, attempts) in reconnect {
            if self.reconnect(nid, addr) {
                debug!(target: "service", "Reconnecting to {nid} (attempts={attempts})...");
            }
        }
    }
}

/// Gives read access to the service state.
pub trait ServiceState {
    /// Get the Node ID.
    fn nid(&self) -> &NodeId;
    /// Get the existing sessions.
    fn sessions(&self) -> &Sessions;
    /// Get fetch state.
    fn fetching(&self) -> &HashMap<RepoId, FetchState>;
    /// Get outbox.
    fn outbox(&self) -> &Outbox;
    /// Get rate limiter.
    fn limiter(&self) -> &RateLimiter;
    /// Get event emitter.
    fn emitter(&self) -> &Emitter<Event>;
    /// Get a repository from storage.
    fn get(&self, rid: RepoId) -> Result<Option<Doc<Verified>>, RepositoryError>;
    /// Get the clock.
    fn clock(&self) -> &LocalTime;
    /// Get the clock mutably.
    fn clock_mut(&mut self) -> &mut LocalTime;
    /// Get service configuration.
    fn config(&self) -> &Config;
    /// Get service metrics.
    fn metrics(&self) -> &Metrics;
}

impl<D, S, G> ServiceState for Service<D, S, G>
where
    D: routing::Store,
    G: Signer,
    S: ReadStorage,
{
    fn nid(&self) -> &NodeId {
        self.signer.public_key()
    }

    fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    fn fetching(&self) -> &HashMap<RepoId, FetchState> {
        &self.fetching
    }

    fn outbox(&self) -> &Outbox {
        &self.outbox
    }

    fn limiter(&self) -> &RateLimiter {
        &self.limiter
    }

    fn emitter(&self) -> &Emitter<Event> {
        &self.emitter
    }

    fn get(&self, rid: RepoId) -> Result<Option<Doc<Verified>>, RepositoryError> {
        self.storage.get(rid)
    }

    fn clock(&self) -> &LocalTime {
        &self.clock
    }

    fn clock_mut(&mut self) -> &mut LocalTime {
        &mut self.clock
    }

    fn config(&self) -> &Config {
        &self.config
    }

    fn metrics(&self) -> &Metrics {
        &self.metrics
    }
}

/// Disconnect reason.
#[derive(Debug)]
pub enum DisconnectReason {
    /// Error while dialing the remote. This error occures before a connection is
    /// even established. Errors of this kind are usually not transient.
    Dial(Arc<dyn std::error::Error + Sync + Send>),
    /// Error with an underlying established connection. Sometimes, reconnecting
    /// after such an error is possible.
    Connection(Arc<dyn std::error::Error + Sync + Send>),
    /// Error with a fetch.
    Fetch(FetchError),
    /// Session error.
    Session(session::Error),
    /// Session conflicts with existing session.
    Conflict,
    /// Connection to self.
    SelfConnection,
    /// User requested disconnect
    Command,
}

impl DisconnectReason {
    pub fn is_dial_err(&self) -> bool {
        matches!(self, Self::Dial(_))
    }

    pub fn is_connection_err(&self) -> bool {
        matches!(self, Self::Connection(_))
    }

    pub fn connection() -> Self {
        DisconnectReason::Connection(Arc::new(std::io::Error::from(
            std::io::ErrorKind::ConnectionReset,
        )))
    }
}

impl fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dial(err) => write!(f, "{err}"),
            Self::Connection(err) => write!(f, "{err}"),
            Self::Command => write!(f, "command"),
            Self::SelfConnection => write!(f, "self-connection"),
            Self::Conflict => write!(f, "conflict"),
            Self::Session(err) => write!(f, "{err}"),
            Self::Fetch(err) => write!(f, "fetch: {err}"),
        }
    }
}

/// Result of a project lookup.
#[derive(Debug)]
pub struct Lookup {
    /// Whether the project was found locally or not.
    pub local: Option<Doc<Verified>>,
    /// A list of remote peers on which the project is known to exist.
    pub remote: Vec<NodeId>,
}

#[derive(thiserror::Error, Debug)]
pub enum LookupError {
    #[error(transparent)]
    Routing(#[from] routing::Error),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Debug, Clone)]
/// Holds currently (or recently) connected peers.
pub struct Sessions(AddressBook<NodeId, Session>);

impl Sessions {
    pub fn new(rng: Rng) -> Self {
        Self(AddressBook::new(rng))
    }

    /// Iterator over fully connected peers.
    pub fn connected(&self) -> impl Iterator<Item = (&NodeId, &Session)> + Clone {
        self.0
            .iter()
            .filter_map(move |(id, sess)| match &sess.state {
                session::State::Connected { .. } => Some((id, sess)),
                _ => None,
            })
    }

    /// Iterator over connected inbound peers.
    pub fn inbound(&self) -> impl Iterator<Item = (&NodeId, &Session)> + Clone {
        self.connected().filter(|(_, s)| s.link.is_inbound())
    }

    /// Iterator over outbound peers.
    pub fn outbound(&self) -> impl Iterator<Item = (&NodeId, &Session)> + Clone {
        self.connected().filter(|(_, s)| s.link.is_outbound())
    }

    /// Iterator over mutable fully connected peers.
    pub fn connected_mut(&mut self) -> impl Iterator<Item = (&NodeId, &mut Session)> {
        self.0.iter_mut().filter(move |(_, s)| s.is_connected())
    }

    /// Iterator over disconnected peers.
    pub fn disconnected_mut(&mut self) -> impl Iterator<Item = (&NodeId, &mut Session)> {
        self.0.iter_mut().filter(move |(_, s)| s.is_disconnected())
    }

    /// Return whether this node has a fully established session.
    pub fn is_connected(&self, id: &NodeId) -> bool {
        self.0.get(id).map(|s| s.is_connected()).unwrap_or(false)
    }

    /// Return whether this node can be connected to.
    pub fn is_disconnected(&self, id: &NodeId) -> bool {
        self.0.get(id).map(|s| s.is_disconnected()).unwrap_or(true)
    }
}

impl Deref for Sessions {
    type Target = AddressBook<NodeId, Session>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Sessions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
