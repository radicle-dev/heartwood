#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::collapsible_if)]
#![warn(clippy::unwrap_used)]
pub mod filter;
pub mod gossip;
pub mod io;
pub mod limitter;
pub mod message;
pub mod session;

use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap, VecDeque};
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
use radicle::node::address::{AddressBook, KnownAddress};
use radicle::node::config::PeerConfig;
use radicle::node::refs::Store as _;
use radicle::node::routing::Store as _;
use radicle::node::seed;
use radicle::node::seed::Store as _;
use radicle::node::{ConnectOptions, Penalty, Severity};
use radicle::storage::refs::SIGREFS_BRANCH;
use radicle::storage::{Inventory, RepositoryError};

use crate::crypto;
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
use crate::service::policy::{store::Write, Policy, Scope};
use crate::storage;
use crate::storage::{refs::RefsAt, Namespaces, ReadStorage};
use crate::worker::fetch;
use crate::worker::FetchError;
use crate::Link;

pub use crate::node::events::{Event, Events};
pub use crate::node::{config::Network, Config, NodeId};
pub use crate::service::message::{Message, ZeroBytes};
pub use crate::service::session::Session;

pub use radicle::node::policy::config as policy;

use self::io::Outbox;
use self::limitter::RateLimiter;
use self::message::InventoryAnnouncement;
use self::policy::NamespacesError;

/// How often to run the "idle" task.
pub const IDLE_INTERVAL: LocalDuration = LocalDuration::from_secs(30);
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
/// Minimum amount of time to wait before reconnecting to a peer.
pub const MIN_RECONNECTION_DELTA: LocalDuration = LocalDuration::from_secs(3);
/// Maximum amount of time to wait before reconnecting to a peer.
pub const MAX_RECONNECTION_DELTA: LocalDuration = LocalDuration::from_mins(60);
/// Connection retry delta used for ephemeral peers that failed to connect previously.
pub const CONNECTION_RETRY_DELTA: LocalDuration = LocalDuration::from_mins(10);
/// How long to wait for a fetch to stall before aborting.
pub const FETCH_TIMEOUT: time::Duration = time::Duration::from_secs(9);

/// Maximum external address limit imposed by message size limits.
pub use message::ADDRESS_LIMIT;
/// Maximum inventory limit imposed by message size limits.
pub use message::INVENTORY_LIMIT;
/// Maximum number of project git references imposed by message size limits.
pub use message::REF_REMOTE_LIMIT;

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
    /// Update local inventory.
    UpdateInventory(RepoId, chan::Sender<bool>),
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
            Self::UpdateInventory(rid, _) => write!(f, "UpdateInventory({rid})"),
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
    #[error("session does not exist; cannot initiate fetch")]
    SessionNotFound,
    #[error("session is not connected; cannot initiate fetch")]
    SessionNotConnected,
    #[error("session fetch capacity reached; cannot initiate fetch")]
    SessionCapacityReached,
    #[error(transparent)]
    Namespaces(#[from] NamespacesError),
}

/// Fetch state for an ongoing fetch.
#[derive(Debug)]
struct FetchState {
    /// Node we're fetching from.
    from: NodeId,
    /// What refs we're fetching.
    refs_at: Vec<RefsAt>,
    /// Channels waiting for fetch results.
    subscribers: Vec<chan::Sender<FetchResult>>,
}

impl FetchState {
    /// Add a subscriber to this fetch.
    fn subscribe(&mut self, c: chan::Sender<FetchResult>) {
        if !self.subscribers.iter().any(|s| s.same_channel(&c)) {
            self.subscribers.push(c);
        }
    }
}

/// Fetch waiting to be processed, in the fetch queue.
#[derive(Debug)]
struct QueuedFetch {
    /// Repo being fetched.
    rid: RepoId,
    /// Peer being fetched from.
    from: NodeId,
    /// Refs being fetched.
    refs_at: Vec<RefsAt>,
    /// Result channel.
    channel: Option<chan::Sender<FetchResult>>,
}

impl PartialEq for QueuedFetch {
    fn eq(&self, other: &Self) -> bool {
        self.rid == other.rid
            && self.from == other.from
            && self.refs_at == other.refs_at
            && self.channel.is_none()
            && other.channel.is_none()
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
    /// I/O outbox.
    outbox: Outbox,
    /// Cached local node announcement.
    node: NodeAnnouncement,
    /// Source of entropy.
    rng: Rng,
    /// Ongoing fetches.
    fetching: HashMap<RepoId, FetchState>,
    /// Fetch queue.
    queue: VecDeque<QueuedFetch>,
    /// Request/connection rate limitter.
    limiter: RateLimiter,
    /// Current seeded repositories bloom filter.
    filter: Filter,
    /// Last time the service was idle.
    last_idle: LocalTime,
    /// Last time the service synced.
    last_sync: LocalTime,
    /// Last time the service routing table was pruned.
    last_prune: LocalTime,
    /// Last time the inventory was announced.
    last_announce: LocalTime,
    /// Last timestamp used for announcements.
    last_timestamp: Timestamp,
    /// Time when the service was initialized, or `None` if it wasn't initialized.
    started_at: Option<LocalTime>,
    /// Publishes events to subscribers.
    emitter: Emitter<Event>,
    /// Local listening addresses.
    listening: Vec<net::SocketAddr>,
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
}

impl<D, S, G> Service<D, S, G>
where
    D: Store,
    S: ReadStorage + 'static,
    G: Signer,
{
    pub fn new(
        config: Config,
        clock: LocalTime,
        db: Stores<D>,
        storage: S,
        policies: policy::Config<Write>,
        signer: G,
        rng: Rng,
        node: NodeAnnouncement,
        emitter: Emitter<Event>,
    ) -> Self {
        let sessions = Sessions::new(rng.clone());

        Self {
            config,
            storage,
            policies,
            signer,
            rng,
            node,
            clock,
            db,
            outbox: Outbox::default(),
            limiter: RateLimiter::default(),
            sessions,
            fetching: HashMap::new(),
            queue: VecDeque::new(),
            filter: Filter::empty(),
            last_idle: LocalTime::default(),
            last_sync: LocalTime::default(),
            last_prune: LocalTime::default(),
            last_timestamp: Timestamp::MIN,
            last_announce: LocalTime::default(),
            started_at: None,
            emitter,
            listening: vec![],
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
        // Nb. This is potentially slow if we have lots of repos. We should probably
        // only re-compute the filter when we've unseeded a certain amount of repos
        // and the filter is really out of date.
        //
        // TODO: Share this code with initialization code.
        self.filter = Filter::new(
            self.policies
                .seed_policies()?
                .filter_map(|t| (t.policy == Policy::Allow).then_some(t.rid)),
        );
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

    /// Lookup a repository, both locally and in the routing table.
    pub fn lookup(&self, rid: RepoId) -> Result<Lookup, LookupError> {
        let remote = self.db.routing().get(&rid)?.iter().cloned().collect();

        Ok(Lookup {
            local: self.storage.get(rid)?,
            remote,
        })
    }

    pub fn initialize(&mut self, time: LocalTime) -> Result<(), Error> {
        debug!(target: "service", "Init @{}", time.as_millis());

        let nid = self.node_id();
        self.started_at = Some(time);

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

        // Ensure that our local node is in our address database.
        self.db
            .addresses_mut()
            .insert(
                &nid,
                self.node.features,
                self.node.alias.clone(),
                self.node.work(),
                self.node.timestamp,
                self.node
                    .addresses
                    .iter()
                    .map(|a| KnownAddress::new(a.clone(), address::Source::Peer)),
            )
            .expect("Service::initialize: error adding local node to address database");

        // Ensure that our inventory is recorded in our routing table, and we are seeding
        // all of it. It can happen that inventory is not properly seeded if for eg. the
        // user creates a new repository while the node is stopped.
        let rids = self.storage.inventory()?;
        self.db.routing_mut().insert(&rids, nid, time.into())?;

        let announced = self
            .db
            .seeds()
            .seeded_by(&nid)?
            .collect::<Result<HashMap<_, _>, _>>()?;
        for rid in rids {
            let repo = self.storage.repository(rid)?;

            // If we're not seeding this repo, just skip it.
            if !self.policies.is_seeding(&rid)? {
                warn!(target: "service", "Local repository {rid} is not seeded");
                continue;
            }
            // If we have no owned refs for this repo, then there's nothing to announce.
            let Ok(updated_at) = SyncedAt::load(&repo, nid) else {
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

        // Setup subscription filter for seeded repos.
        self.filter = Filter::new(
            self.policies
                .seed_policies()?
                .filter_map(|t| (t.policy == Policy::Allow).then_some(t.rid)),
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

        Ok(())
    }

    pub fn tick(&mut self, now: LocalTime) {
        trace!(
            target: "service",
            "Tick +{}",
            now - self.started_at.expect("Service::tick: service must be initialized")
        );
        self.clock = now;
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
            self.maintain_connections();
            self.outbox.wakeup(IDLE_INTERVAL);
            self.last_idle = now;
        }
        if now - self.last_sync >= SYNC_INTERVAL {
            trace!(target: "service", "Running 'sync' task...");

            if let Err(e) = self.fetch_missing_inventory() {
                error!(target: "service", "Error fetching missing inventory: {e}");
            }
            self.outbox.wakeup(SYNC_INTERVAL);
            self.last_sync = now;
        }
        if now - self.last_announce >= ANNOUNCE_INTERVAL {
            trace!(target: "service", "Running 'announce' task...");

            if let Err(err) = self
                .storage
                .inventory()
                .and_then(|i| self.announce_inventory(i))
            {
                error!(target: "service", "Error announcing inventory: {err}");
            }
            self.outbox.wakeup(ANNOUNCE_INTERVAL);
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
                if let Err(err) = self
                    .storage
                    .inventory()
                    .and_then(|i| self.announce_inventory(i))
                {
                    error!(target: "service", "Error announcing inventory: {err}");
                }
            }
            Command::UpdateInventory(rid, resp) => {
                self.storage.insert(rid);

                let synced = self
                    .sync_inventory()
                    .expect("Service::command: error syncing inventory");
                resp.send(synced.added.len() + synced.removed.len() > 0)
                    .ok();
            }
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
                    self._fetch(rid, from, status.want, timeout, channel);
                    return true;
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
    ) {
        self._fetch(rid, from, vec![], timeout, channel)
    }

    fn _fetch(
        &mut self,
        rid: RepoId,
        from: NodeId,
        refs_at: Vec<RefsAt>,
        timeout: time::Duration,
        channel: Option<chan::Sender<FetchResult>>,
    ) {
        match self.try_fetch(rid, &from, refs_at.clone(), timeout) {
            Ok(fetching) => {
                if let Some(c) = channel {
                    fetching.subscribe(c);
                }
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
                        channel,
                    };
                    if self.queue.contains(&fetch) {
                        debug!(target: "service", "Fetch for {rid} with {from} is already queued..");
                    } else {
                        debug!(target: "service", "Queueing fetch for {rid} with {from}..");
                        self.queue.push_back(fetch);
                    }
                }
            }
            Err(TryFetchError::SessionCapacityReached) => {
                debug!(target: "service", "Fetch capacity reached for {from}, queueing {rid}..");
                self.queue.push_back(QueuedFetch {
                    rid,
                    refs_at,
                    from,
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
            return Err(TryFetchError::SessionNotFound);
        };
        let fetching = self.fetching.entry(rid);

        trace!(target: "service", "Trying to fetch {refs_at:?} for {rid}..");

        if let Entry::Occupied(fetching) = fetching {
            // We're already fetching this repo from some peer.
            return Err(TryFetchError::AlreadyFetching(fetching.into_mut()));
        }
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

        let fetching = fetching.or_insert(FetchState {
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

                    self.storage.insert(rid);
                    self.sync_and_announce_inventory();
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
        // We can now try to dequeue another fetch.
        self.dequeue_fetch();
    }

    /// Fetches are queued for two reasons:
    /// 1. The RID was already being fetched.
    /// 2. The session was already at fetch capacity.
    pub fn dequeue_fetch(&mut self) {
        while let Some(QueuedFetch {
            rid,
            from,
            refs_at,
            channel,
        }) = self.queue.pop_front()
        {
            debug!(target: "service", "Dequeued fetch for {rid} from session {from}..");

            if let Some(refs) = NonEmpty::from_vec(refs_at) {
                let repo_entry = self
                    .policies
                    .seed_policy(&rid)
                    .expect("Service::dequeue_fetch: error accessing repo seeding configuration");

                // Keep dequeueing if there was nothing to fetch, otherwise break.
                if self.fetch_refs_at(rid, from, refs, repo_entry.scope, FETCH_TIMEOUT, channel) {
                    break;
                }
            } else {
                // If no refs are specified, always do a full fetch.
                self.fetch(rid, from, FETCH_TIMEOUT, channel);
                break;
            }
        }
    }

    /// Inbound connection attempt.
    pub fn accepted(&mut self, addr: Address) -> bool {
        // Always accept trusted connections, even if we already reached
        // our inbound connection limit.
        if addr.is_trusted() {
            return true;
        }
        // Check for inbound connection limit.
        if self.sessions.inbound().count() >= self.config.limits.connection.inbound {
            return false;
        }
        let host: HostName = addr.into();

        if self
            .limiter
            .limit(host.clone(), &self.config.limits.rate.inbound, self.clock)
        {
            trace!(target: "service", "Rate limitting inbound connection from {host}..");
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
        info!(target: "service", "Connected to {} ({:?})", remote, link);
        self.emitter.emit(Event::PeerConnected { nid: remote });

        let msgs = self.initial(link);

        if link.is_outbound() {
            if let Some(peer) = self.sessions.get_mut(&remote) {
                peer.to_connected(self.clock);
                self.outbox.write_all(peer, msgs);

                if let Err(e) =
                    self.db
                        .addresses_mut()
                        .connected(&remote, &peer.addr, self.clock.into())
                {
                    error!(target: "service", "Error updating address book with connection: {e}");
                }
            }
        } else {
            match self.sessions.entry(remote) {
                Entry::Occupied(e) => {
                    warn!(
                        target: "service",
                        "Connecting peer {remote} already has a session open ({})", e.get()
                    );
                }
                Entry::Vacant(e) => {
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
        self.dequeue_fetch();
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
        relayer_addr: &Address,
        announcement: &Announcement,
    ) -> Result<bool, session::Error> {
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
            return Ok(false);
        }
        let now = self.clock;
        let timestamp = message.timestamp();
        // To avoid spamming peers on startup with historical gossip messages,
        // don't relay messages that are too old.
        let relay = if now - timestamp.to_local_time() > MAX_TIME_DELTA {
            false
        } else {
            self.config.relay
        };

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
                        return Ok(false);
                    }
                }
                Err(e) => {
                    error!(target: "service", "Error looking up node in address book: {e}");
                    return Ok(false);
                }
            }
        }

        // Discard announcement messages we've already seen, otherwise update our last seen time.
        match self.db.gossip_mut().announced(announcer, announcement) {
            Ok(fresh) => {
                if !fresh {
                    debug!(target: "service", "Ignoring stale announcement from {announcer} (t={timestamp})");
                    return Ok(false);
                }
            }
            Err(e) => {
                error!(target: "service", "Error updating gossip entry from {announcer}: {e}");
                return Ok(false);
            }
        }

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
                            return Ok(false);
                        }
                    }
                    Err(e) => {
                        error!(target: "service", "Error processing inventory from {announcer}: {e}");
                        return Ok(false);
                    }
                }

                for id in message.inventory.as_slice() {
                    // TODO: Move this out (good luck with the borrow checker).
                    if let Some(sess) = self.sessions.get_mut(announcer) {
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
                            match self.storage.contains(id) {
                                Ok(true) => {
                                    // Do nothing.
                                }
                                Ok(false) => {
                                    debug!(target: "service", "Missing seeded inventory {id}; initiating fetch..");
                                    self.fetch(*id, *announcer, FETCH_TIMEOUT, None);
                                }
                                Err(e) => {
                                    error!(target: "service", "Error checking local inventory: {e}");
                                }
                            }
                        }
                    }
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
                    return Ok(false);
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
                if repo_entry.policy != Policy::Allow {
                    debug!(
                        target: "service",
                        "Ignoring refs announcement from {announcer}: repository {} isn't seeded (t={timestamp})",
                        message.rid
                    );
                    return Ok(false);
                }
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
                self.fetch_refs_at(
                    message.rid,
                    remote.id,
                    refs,
                    repo_entry.scope,
                    FETCH_TIMEOUT,
                    None,
                );
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
                    *features,
                    ann.alias.clone(),
                    ann.work(),
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
        Ok(false)
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
            .limit(peer.addr.clone().into(), limit, self.clock)
        {
            trace!(target: "service", "Rate limiting message from {remote} ({})", peer.addr);
            return Ok(());
        }
        message.log(log::Level::Debug, remote, Link::Inbound);

        trace!(target: "service", "Received message {:?} from {}", &message, peer.id);

        match (&mut peer.state, message) {
            // Process a peer announcement.
            (session::State::Connected { .. }, Message::Announcement(ann)) => {
                let relayer = peer.id;
                let relayer_addr = peer.addr.clone();
                let announcer = ann.node;

                // Returning true here means that the message should be relayed.
                if self.handle_announcement(&relayer_addr, &ann)? {
                    // Choose peers we should relay this message to.
                    // 1. Don't relay to the peer who sent us this message.
                    // 2. Don't relay to the peer who signed this announcement.
                    let relay_to = self
                        .sessions
                        .connected()
                        .filter(|(id, _)| *id != &relayer && *id != &announcer)
                        .map(|(_, p)| p);

                    self.outbox.relay(ann, relay_to);

                    return Ok(());
                }
            }
            (session::State::Connected { .. }, Message::Subscribe(subscribe)) => {
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
                            self.outbox.write(peer, ann.into());
                        }
                    }
                    Err(e) => {
                        error!(target: "service", "Error querying gossip messages from store: {e}");
                    }
                }
                peer.subscribe = Some(subscribe);
            }
            (session::State::Connected { .. }, Message::Info(info)) => {
                let remote = peer.id;
                self.handle_info(remote, &info)?;
            }
            (session::State::Connected { .. }, Message::Ping(Ping { ponglen, .. })) => {
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
            (
                session::State::Connected {
                    ping, latencies, ..
                },
                Message::Pong { zeroes },
            ) => {
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
            (session::State::Attempted { .. } | session::State::Initial, msg) => {
                debug!(target: "service", "Ignoring unexpected message {:?} from connecting peer {}", msg, peer.id);
            }
            (session::State::Disconnected { .. }, msg) => {
                debug!(target: "service", "Ignoring {:?} from disconnected peer {}", msg, peer.id);
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
        if let Ok(result) = self.db.routing_mut().insert([&rid], nid, time) {
            if let &[(_, InsertResult::SeedAdded)] = result.as_slice() {
                self.emitter.emit(Event::SeedDiscovered { rid, nid });
                info!(target: "service", "Routing table updated for {} with seed {nid}", rid);
            }
        }
    }

    /// Set of initial messages to send to a peer.
    fn initial(&mut self, _link: Link) -> Vec<Message> {
        let timestamp = self.timestamp();
        let now = self.clock();
        let filter = self.filter();
        let inventory = match self.storage.inventory() {
            Ok(i) => i,
            Err(e) => {
                // Other than crashing the node completely, there's nothing we can do
                // here besides returning an empty inventory and logging an error.
                error!(target: "service", "Error getting local inventory for initial messages: {e}");
                Default::default()
            }
        };

        // TODO: Only subscribe to outbound connections, otherwise we will consume too
        // much bandwidth.

        // If we've been previously connected to the network, we'll have received gossip messages.
        // Instead of simply taking the last timestamp we try to ensure we don't miss any
        // messages due un-synchronized clocks.
        //
        // If this is our first connection to the network, we just ask for a fixed backlog
        // of messages to get us started.
        let since = match self.db.gossip().last() {
            Ok(Some(last)) => Timestamp::from(last.to_local_time() - MAX_TIME_DELTA),
            Ok(None) => (*now - INITIAL_SUBSCRIBE_BACKLOG_DELTA).into(),
            Err(e) => {
                error!(target: "service", "Error getting the lastest gossip message from storage: {e}");
                return vec![];
            }
        };

        debug!(target: "service", "Subscribing to messages since timestamp {since}..");

        vec![
            Message::node(self.node.clone(), &self.signer),
            Message::inventory(gossip::inventory(timestamp, inventory), &self.signer),
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

    /// Update our routing table with our local node's inventory.
    fn sync_inventory(&mut self) -> Result<SyncedRouting, Error> {
        let inventory = self.storage.inventory()?;
        let result = self.sync_routing(inventory, self.node_id(), self.clock.into())?;

        Ok(result)
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

        for (rid, result) in self
            .db
            .routing_mut()
            .insert(included.iter(), from, timestamp)?
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
        for rid in self.db.routing().get_resources(&from)?.into_iter() {
            if !included.contains(&rid) {
                if self.db.routing_mut().remove(&rid, &from)? {
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
        // TODO: Move to `announce_own_refs`.
        if let Some(refs) = refs.iter().find(|r| r.remote == ann.node) {
            info!(
                target: "service",
                "Announcing own refs for {rid} to peers ({}) (t={timestamp})..",
                refs.at
            );

            if let Err(e) = self
                .db
                .seeds_mut()
                .synced(&rid, &ann.node, refs.at, timestamp)
            {
                error!(target: "service", "Error updating sync status for local node: {e}");
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

    fn sync_and_announce_inventory(&mut self) {
        match self.sync_inventory() {
            Ok(synced) => {
                // Only announce if our inventory changed.
                if synced.added.len() + synced.removed.len() > 0 {
                    if let Err(e) = self
                        .storage
                        .inventory()
                        .and_then(|i| self.announce_inventory(i))
                    {
                        error!(target: "service", "Failed to announce inventory: {e}");
                    }
                }
            }
            Err(e) => {
                error!(target: "service", "Failed to sync inventory: {e}");
            }
        }
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
        if self.config.policy == Policy::Allow {
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

    ////////////////////////////////////////////////////////////////////////////
    // Periodic tasks
    ////////////////////////////////////////////////////////////////////////////

    /// Announce our inventory to all connected peers.
    fn announce_inventory(&mut self, inventory: Inventory) -> Result<(), storage::Error> {
        let time = self.timestamp();
        let msg = AnnouncementMessage::from(gossip::inventory(time, inventory));

        self.outbox.announce(
            msg.signed(&self.signer),
            self.sessions.connected().map(|(_, p)| p),
            self.db.gossip_mut(),
        );
        self.last_announce = time.to_local_time();

        Ok(())
    }

    fn prune_routing_entries(&mut self, now: &LocalTime) -> Result<(), routing::Error> {
        let count = self.db.routing().len()?;
        if count <= self.config.limits.routing_max_size {
            return Ok(());
        }

        let delta = count - self.config.limits.routing_max_size;
        self.db.routing_mut().prune(
            (*now - self.config.limits.routing_max_age).into(),
            Some(delta),
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
                    .filter(|entry| !entry.address.banned)
                    .filter(|entry| !entry.penalty.is_threshold_reached())
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

    /// Fetch all repositories that are seeded but missing from our inventory.
    fn fetch_missing_inventory(&mut self) -> Result<(), Error> {
        let inventory = self.storage().inventory()?;
        let missing = self
            .policies
            .seed_policies()?
            .filter_map(|t| (t.policy == Policy::Allow).then_some(t.rid))
            .filter(|rid| !inventory.contains(rid));

        for rid in missing {
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

    fn maintain_connections(&mut self) {
        let PeerConfig::Dynamic { target } = self.config.peers else {
            return;
        };
        trace!(target: "service", "Maintaining connections..");

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
            .take(wanted)
            .collect::<Vec<_>>();

        if available.len() < target {
            log::warn!(
                target: "service",
                "Not enough available peers to connect to (available={}, target={target})",
                available.len()
            );
        }
        for (id, ka) in available {
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
    /// Get a repository from storage.
    fn get(&self, rid: RepoId) -> Result<Option<Doc<Verified>>, RepositoryError>;
    /// Get the clock.
    fn clock(&self) -> &LocalTime;
    /// Get the clock mutably.
    fn clock_mut(&mut self) -> &mut LocalTime;
    /// Get service configuration.
    fn config(&self) -> &Config;
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
