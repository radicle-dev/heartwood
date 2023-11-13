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
use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::{fmt, time};

use crossbeam_channel as chan;
use fastrand::Rng;
use localtime::{LocalDuration, LocalTime};
use log::*;
use nonempty::NonEmpty;

use radicle::node::address;
use radicle::node::address::{AddressBook, KnownAddress};
use radicle::node::config::PeerConfig;
use radicle::node::ConnectOptions;
use radicle::storage::RepositoryError;

use crate::crypto;
use crate::crypto::{Signer, Verified};
use crate::identity::{Doc, Id};
use crate::node::routing;
use crate::node::routing::InsertResult;
use crate::node::{Address, Alias, Features, FetchResult, HostName, Seed, Seeds, SyncStatus};
use crate::prelude::*;
use crate::runtime::Emitter;
use crate::service::message::{Announcement, AnnouncementMessage, Ping};
use crate::service::message::{NodeAnnouncement, RefsAnnouncement};
use crate::service::tracking::{store::Write, Scope};
use crate::storage;
use crate::storage::refs::RefsAt;
use crate::storage::ReadRepository;
use crate::storage::{Namespaces, ReadStorage};
use crate::worker::fetch;
use crate::worker::FetchError;
use crate::Link;

pub use crate::node::events::{Event, Events};
pub use crate::node::{config::Network, Config, NodeId};
pub use crate::service::message::{Message, ZeroBytes};
pub use crate::service::session::Session;

pub use radicle::node::tracking::config as tracking;

use self::io::Outbox;
use self::limitter::RateLimiter;
use self::message::InventoryAnnouncement;
use self::tracking::NamespacesError;

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
/// Maximum time difference between the local time, and an announcement timestamp.
pub const MAX_TIME_DELTA: LocalDuration = LocalDuration::from_mins(60);
/// Maximum attempts to connect to a peer before we give up.
pub const MAX_CONNECTION_ATTEMPTS: usize = 3;
/// How far back from the present time should we request gossip messages when connecting to a peer,
/// when we initially come online.
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
    added: Vec<Id>,
    /// Repo entries removed.
    removed: Vec<Id>,
    /// Repo entries updated (time).
    updated: Vec<Id>,
}

impl SyncedRouting {
    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.updated.is_empty()
    }
}

/// General service error.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Git(#[from] radicle::git::raw::Error),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Refs(#[from] storage::refs::Error),
    #[error(transparent)]
    Routing(#[from] routing::Error),
    #[error(transparent)]
    Addresses(#[from] address::Error),
    #[error(transparent)]
    Tracking(#[from] tracking::Error),
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
    #[error("namespaces error: {0}")]
    Namespaces(#[from] NamespacesError),
}

/// Function used to query internal service state.
pub type QueryState = dyn Fn(&dyn ServiceState) -> Result<(), CommandError> + Send + Sync;

/// Commands sent to the service by the operator.
pub enum Command {
    /// Announce repository references for given repository to peers.
    AnnounceRefs(Id),
    /// Announce local repositories to peers.
    AnnounceInventory,
    /// Announce local inventory to peers.
    SyncInventory(chan::Sender<bool>),
    /// Connect to node with the given address.
    Connect(NodeId, Address, ConnectOptions),
    /// Disconnect from node.
    Disconnect(NodeId),
    /// Get the node configuration.
    Config(chan::Sender<Config>),
    /// Lookup seeds for the given repository in the routing table.
    Seeds(Id, chan::Sender<Seeds>),
    /// Fetch the given repository from the network.
    Fetch(Id, NodeId, time::Duration, chan::Sender<FetchResult>),
    /// Track the given repository.
    TrackRepo(Id, Scope, chan::Sender<bool>),
    /// Untrack the given repository.
    UntrackRepo(Id, chan::Sender<bool>),
    /// Track the given node.
    TrackNode(NodeId, Option<Alias>, chan::Sender<bool>),
    /// Untrack the given node.
    UntrackNode(NodeId, chan::Sender<bool>),
    /// Query the internal service state.
    QueryState(Arc<QueryState>, chan::Sender<Result<(), CommandError>>),
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnnounceRefs(id) => write!(f, "AnnounceRefs({id})"),
            Self::AnnounceInventory => write!(f, "AnnounceInventory"),
            Self::SyncInventory(_) => write!(f, "SyncInventory(..)"),
            Self::Connect(id, addr, opts) => write!(f, "Connect({id}, {addr}, {opts:?})"),
            Self::Disconnect(id) => write!(f, "Disconnect({id})"),
            Self::Config(_) => write!(f, "Config"),
            Self::Seeds(id, _) => write!(f, "Seeds({id})"),
            Self::Fetch(id, node, _, _) => write!(f, "Fetch({id}, {node})"),
            Self::TrackRepo(id, scope, _) => write!(f, "TrackRepo({id}, {scope})"),
            Self::UntrackRepo(id, _) => write!(f, "UntrackRepo({id})"),
            Self::TrackNode(id, _, _) => write!(f, "TrackNode({id})"),
            Self::UntrackNode(id, _) => write!(f, "UntrackNode({id})"),
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
    Tracking(#[from] tracking::Error),
}

#[derive(Debug)]
pub struct Service<R, A, S, G> {
    /// Service configuration.
    config: Config,
    /// Our cryptographic signer and key.
    signer: G,
    /// Project storage.
    storage: S,
    /// Network routing table. Keeps track of where projects are located.
    routing: R,
    /// Node address manager.
    addresses: A,
    /// Tracking policy configuration.
    tracking: tracking::Config<Write>,
    /// State relating to gossip.
    gossip: gossip::Store,
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
    /// Fetch requests initiated by user, which are waiting for results.
    fetch_reqs: HashMap<(Id, NodeId), chan::Sender<FetchResult>>,
    /// Request/connection rate limitter.
    limiter: RateLimiter,
    /// Current tracked repository bloom filter.
    filter: Filter,
    /// Last time the service was idle.
    last_idle: LocalTime,
    /// Last time the service synced.
    last_sync: LocalTime,
    /// Last time the service routing table was pruned.
    last_prune: LocalTime,
    /// Last time the service announced its inventory.
    last_announce: LocalTime,
    /// Time when the service was initialized.
    start_time: LocalTime,
    /// Publishes events to subscribers.
    emitter: Emitter<Event>,
}

impl<R, A, S, G> Service<R, A, S, G>
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

impl<R, A, S, G> Service<R, A, S, G>
where
    R: routing::Store,
    A: address::Store,
    S: ReadStorage + 'static,
    G: Signer,
{
    pub fn new(
        config: Config,
        clock: LocalTime,
        routing: R,
        storage: S,
        addresses: A,
        gossip: gossip::Store,
        tracking: tracking::Config<Write>,
        signer: G,
        rng: Rng,
        node: NodeAnnouncement,
        emitter: Emitter<Event>,
    ) -> Self {
        let sessions = Sessions::new(rng.clone());

        Self {
            config,
            storage,
            addresses,
            tracking,
            signer,
            rng,
            node,
            clock,
            routing,
            gossip,
            outbox: Outbox::default(),
            limiter: RateLimiter::default(),
            sessions,
            fetch_reqs: HashMap::new(),
            filter: Filter::empty(),
            last_idle: LocalTime::default(),
            last_sync: LocalTime::default(),
            last_prune: LocalTime::default(),
            last_announce: LocalTime::default(),
            start_time: LocalTime::default(),
            emitter,
        }
    }

    /// Return the next i/o action to execute.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<io::Io> {
        self.outbox.next()
    }

    /// Track a repository.
    /// Returns whether or not the tracking policy was updated.
    pub fn track_repo(&mut self, id: &Id, scope: Scope) -> Result<bool, tracking::Error> {
        let updated = self.tracking.track_repo(id, scope)?;
        self.filter.insert(id);

        Ok(updated)
    }

    /// Untrack a repository.
    /// Returns whether or not the tracking policy was updated.
    /// Note that when untracking, we don't announce anything to the network. This is because by
    /// simply not announcing it anymore, it will eventually be pruned by nodes.
    pub fn untrack_repo(&mut self, id: &Id) -> Result<bool, tracking::Error> {
        let updated = self.tracking.untrack_repo(id)?;
        // Nb. This is potentially slow if we have lots of projects. We should probably
        // only re-compute the filter when we've untracked a certain amount of projects
        // and the filter is really out of date.
        //
        // TODO: Share this code with initialization code.
        self.filter = Filter::new(
            self.tracking
                .repo_policies()?
                .filter_map(|t| (t.policy == tracking::Policy::Track).then_some(t.id)),
        );
        Ok(updated)
    }

    /// Check whether we are tracking a certain repository.
    pub fn is_tracking(&self, id: &Id) -> Result<bool, tracking::Error> {
        self.tracking.is_repo_tracked(id)
    }

    /// Find the closest `n` peers by proximity in tracking graphs.
    /// Returns a sorted list from the closest peer to the furthest.
    /// Peers with more trackings in common score score higher.
    #[allow(unused)]
    pub fn closest_peers(&self, n: usize) -> Vec<NodeId> {
        todo!()
    }

    /// Get the address book instance.
    pub fn addresses(&self) -> &A {
        &self.addresses
    }

    /// Get the mutable address book instance.
    pub fn addresses_mut(&mut self) -> &mut A {
        &mut self.addresses
    }

    /// Get the routing store.
    pub fn routing(&self) -> &R {
        &self.routing
    }

    /// Get the storage instance.
    pub fn storage(&self) -> &S {
        &self.storage
    }

    /// Get the mutable storage instance.
    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    /// Get the tracking policy.
    pub fn tracking(&self) -> &tracking::Config<Write> {
        &self.tracking
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

    /// Lookup a project, both locally and in the routing table.
    pub fn lookup(&self, rid: Id) -> Result<Lookup, LookupError> {
        let remote = self.routing.get(&rid)?.iter().cloned().collect();

        Ok(Lookup {
            local: self.storage.get(rid)?,
            remote,
        })
    }

    pub fn initialize(&mut self, time: LocalTime) -> Result<(), Error> {
        debug!(target: "service", "Init @{}", time.as_millis());

        self.start_time = time;

        // Connect to configured peers.
        let addrs = self.config.connect.clone();
        for (id, addr) in addrs.into_iter().map(|ca| ca.into()) {
            self.connect(id, addr);
        }
        // Ensure that our inventory is recorded in our routing table, and we are tracking
        // all of it. It can happen that inventory is not properly tracked if for eg. the
        // user creates a new repository while the node is stopped.
        let rids = self.storage.inventory()?;
        self.routing
            .insert(&rids, self.node_id(), time.as_millis())?;

        for rid in rids {
            if !self.is_tracking(&rid)? {
                warn!(target: "service", "Local repository {rid} is not tracked");
            }
        }
        // Ensure that our local node is in our address database.
        self.addresses
            .insert(
                &self.node_id(),
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

        // Setup subscription filter for tracked repos.
        self.filter = Filter::new(
            self.tracking
                .repo_policies()?
                .filter_map(|t| (t.policy == tracking::Policy::Track).then_some(t.id)),
        );
        // Try to establish some connections.
        self.maintain_connections();
        // Start periodic tasks.
        self.outbox.wakeup(IDLE_INTERVAL);

        Ok(())
    }

    pub fn tick(&mut self, now: LocalTime) {
        trace!(target: "service", "Tick +{}", now - self.start_time);

        self.clock = now;
    }

    pub fn wake(&mut self) {
        let now = self.clock;

        trace!(target: "service", "Wake +{}", now - self.start_time);

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
            if let Err(err) = self
                .storage
                .inventory()
                .and_then(|i| self.announce_inventory(i))
            {
                error!(target: "service", "Error announcing inventory: {err}");
            }
            self.outbox.wakeup(ANNOUNCE_INTERVAL);
            self.last_announce = now;
        }
        if now - self.last_prune >= PRUNE_INTERVAL {
            trace!(target: "service", "Running 'prune' task...");

            if let Err(err) = self.prune_routing_entries(&now) {
                error!(target: "service", "Error pruning routing entries: {err}");
            }
            if let Err(err) = self
                .gossip
                .prune((now - self.config.limits.gossip_max_age).as_millis())
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
                // TODO: Establish connections to unconnected seeds, and retry.
                self.fetch_reqs.insert((rid, seed), resp);
                self.fetch(rid, &seed, timeout);
            }
            Command::TrackRepo(rid, scope, resp) => {
                // Update our tracking policy.
                let tracked = self
                    .track_repo(&rid, scope)
                    .expect("Service::command: error tracking repository");
                resp.send(tracked).ok();

                // Let all our peers know that we're interested in this repo from now on.
                self.outbox.broadcast(
                    Message::subscribe(self.filter(), self.time(), Timestamp::MAX),
                    self.sessions.connected().map(|(_, s)| s),
                );
            }
            Command::UntrackRepo(id, resp) => {
                let untracked = self
                    .untrack_repo(&id)
                    .expect("Service::command: error untracking repository");
                resp.send(untracked).ok();
            }
            Command::TrackNode(id, alias, resp) => {
                let tracked = self
                    .tracking
                    .track_node(&id, alias.as_deref())
                    .expect("Service::command: error tracking node");
                resp.send(tracked).ok();
            }
            Command::UntrackNode(id, resp) => {
                let untracked = self
                    .tracking
                    .untrack_node(&id)
                    .expect("Service::command: error untracking node");
                resp.send(untracked).ok();
            }
            Command::AnnounceRefs(id) => {
                if let Err(err) = self.announce_refs(id, [self.node_id()]) {
                    error!("Error announcing refs: {}", err);
                }
            }
            Command::AnnounceInventory => {
                if let Err(err) = self
                    .storage
                    .inventory()
                    .and_then(|i| self.announce_inventory(i))
                {
                    error!("Error announcing inventory: {}", err);
                }
            }
            Command::SyncInventory(resp) => {
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

    /// Initiate an outgoing fetch for some repository, based on
    /// another node's announcement.
    fn fetch_refs_at(
        &mut self,
        rid: Id,
        from: &NodeId,
        refs: Vec<RefsAt>,
        timeout: time::Duration,
    ) {
        self._fetch(rid, from, refs, timeout)
    }

    /// Initiate an outgoing fetch for some repository.
    fn fetch(&mut self, rid: Id, from: &NodeId, timeout: time::Duration) {
        self._fetch(rid, from, vec![], timeout)
    }

    fn _fetch(&mut self, rid: Id, from: &NodeId, refs_at: Vec<RefsAt>, timeout: time::Duration) {
        let Some(session) = self.sessions.get_mut(from) else {
            error!(target: "service", "Session {from} does not exist; cannot initiate fetch");
            return;
        };
        if !session.is_connected() {
            // This can happen if a session disconnects in the time between asking for seeds to
            // fetch from, and initiating the fetch from one of those seeds.
            error!(target: "service", "Session {from} is not connected; cannot initiate fetch");
            return;
        }
        let seed = session.id;

        match session.fetch(rid) {
            session::FetchResult::Queued => {
                debug!(target: "service", "Fetch queued for {rid} with {seed}..");
            }
            session::FetchResult::Ready => {
                debug!(target: "service", "Fetch initiated for {rid} with {seed}..");
                match self.tracking.namespaces_for(&self.storage, &rid) {
                    Ok(namespaces) => {
                        self.outbox
                            .fetch(session, rid, namespaces, refs_at, timeout);
                    }
                    Err(err) => {
                        error!(target: "service", "Error getting namespaces for {rid}: {err}");

                        if let Some(resp) = self.fetch_reqs.remove(&(rid, seed)) {
                            resp.send(FetchResult::Failed {
                                reason: err.to_string(),
                            })
                            .ok();
                        }
                    }
                };
            }
            session::FetchResult::AlreadyFetching => {
                debug!(target: "service", "Ignoring redundant attempt to fetch {rid} from {from}");
            }
            session::FetchResult::NotConnected => {
                error!(target: "service", "Unable to fetch {rid} from peer {seed}: peer is not connected");
            }
        }
    }

    pub fn fetched(
        &mut self,
        rid: Id,
        remote: NodeId,
        result: Result<fetch::FetchResult, FetchError>,
    ) {
        let result = match result {
            Ok(fetch::FetchResult {
                updated,
                namespaces,
            }) => {
                debug!(target: "service", "Fetched {rid} from {remote} successfully");

                for update in &updated {
                    debug!(target: "service", "Ref updated: {update} for {rid}");
                }
                self.emitter.emit(Event::RefsFetched {
                    remote,
                    rid,
                    updated: updated.clone(),
                });

                FetchResult::Success {
                    updated,
                    namespaces,
                }
            }
            Err(err) => {
                let reason = err.to_string();
                error!(target: "service", "Fetch failed for {rid} from {remote}: {reason}");

                // For now, we only disconnect the remote in case of timeout. In the future,
                // there may be other reasons to disconnect.
                if err.is_timeout() {
                    self.outbox.disconnect(remote, DisconnectReason::Fetch(err));
                }
                FetchResult::Failed { reason }
            }
        };

        if let Some(results) = self.fetch_reqs.remove(&(rid, remote)) {
            debug!(target: "service", "Found existing fetch request, sending result..");

            if results.send(result).is_err() {
                error!(target: "service", "Error sending fetch result for {rid}..");
            } else {
                debug!(target: "service", "Sent fetch result for {rid}..");
            }
        } else {
            debug!(target: "service", "No fetch requests found for {rid}..");

            // We only announce refs here when the fetch wasn't user-requested. This is
            // because the user might want to announce his fork, once he has created one,
            // or may choose to not announce anything.
            match result {
                FetchResult::Success {
                    updated,
                    namespaces,
                } if !updated.is_empty() => {
                    if let Err(e) = self.announce_refs(rid, namespaces) {
                        error!(target: "service", "Failed to announce new refs: {e}");
                    }
                }
                _ => debug!(target: "service", "Nothing to announce, no refs were updated.."),
            }
        }
        // TODO: Since this fetch could be either a full clone
        // or simply a ref update, we need to either announce
        // new inventory, or new refs. Right now, we announce
        // both in some cases.
        //
        // Announce the newly fetched repository to the
        // network, if necessary.
        self.sync_and_announce();

        if let Some(s) = self.sessions.get_mut(&remote) {
            if let Some(dequeued) = s.fetched(rid) {
                debug!(target: "service", "Dequeued fetch {dequeued} from session {remote}..");

                self.fetch(dequeued, &remote, FETCH_TIMEOUT);
            }
        }
    }

    /// Inbound connection attempt.
    pub fn accepted(&mut self, addr: Address) -> bool {
        // Always accept trusted connections.
        if addr.is_trusted() {
            return true;
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

    pub fn connected(&mut self, remote: NodeId, addr: Address, link: Link) {
        info!(target: "service", "Connected to {} ({:?})", remote, link);
        self.emitter.emit(Event::PeerConnected { nid: remote });

        let msgs = self.initial(link);
        let now = self.time();

        if link.is_outbound() {
            if let Some(peer) = self.sessions.get_mut(&remote) {
                peer.to_connected(self.clock);
                self.outbox.write_all(peer, msgs);

                if let Err(e) = self.addresses.connected(&remote, &peer.addr, now) {
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

    pub fn disconnected(&mut self, remote: NodeId, reason: &DisconnectReason) {
        let since = self.local_time();

        debug!(target: "service", "Disconnected from {} ({})", remote, reason);
        self.emitter.emit(Event::PeerDisconnected {
            nid: remote,
            reason: reason.to_string(),
        });

        let Some(session) = self.sessions.get_mut(&remote) else {
            if cfg!(debug_assertions) {
                panic!("Service::disconnected: unknown session {remote}");
            } else {
                return;
            }
        };
        let link = session.link;

        // If the peer disconnected while we were fetching, return a failure to any
        // potential fetcher.
        for rid in session.fetching() {
            if let Some(resp) = self.fetch_reqs.remove(&(rid, remote)) {
                resp.send(FetchResult::Failed {
                    reason: format!("disconnected: {reason}"),
                })
                .ok();
            }
        }

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

            if let Err(e) =
                self.addresses
                    .disconnected(&remote, &session.addr, reason.is_transient())
            {
                error!(target: "service", "Error updating address store: {e}");
            }
            self.sessions.remove(&remote);
            // Only re-attempt outbound connections, since we don't care if an inbound connection
            // is dropped.
            if link.is_outbound() {
                self.maintain_connections();
            }
        }
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
        if *announcer == self.node_id() {
            return Ok(false);
        }
        let now = self.clock;
        let timestamp = message.timestamp();
        let relay = self.config.relay;

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
            match self.addresses.get(announcer) {
                Ok(node) => {
                    if node.is_none() {
                        debug!(target: "service", "Ignoring announcement from unknown node {announcer}");
                        return Ok(false);
                    }
                }
                Err(e) => {
                    error!(target: "service", "Error looking up node in address book: {e}");
                    return Ok(false);
                }
            }
        }

        // Discard announcement messages we've already seen, otherwise update out last seen time.
        match self.gossip.announced(announcer, announcement) {
            Ok(fresh) => {
                if !fresh {
                    trace!(target: "service", "Ignoring stale inventory announcement from {announcer} (t={})", self.time());
                    return Ok(false);
                }
            }
            Err(e) => {
                error!(target: "service", "Error updating gossip entry from {announcer}: {e}");
                return Ok(false);
            }
        }

        match message {
            AnnouncementMessage::Inventory(message) => {
                match self.sync_routing(&message.inventory, *announcer, message.timestamp) {
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

                        // If we're tracking and connected to the announcer, and we don't have
                        // the inventory, fetch it from the announcer.
                        if self.tracking.is_repo_tracked(id).expect(
                            "Service::handle_announcement: error accessing tracking configuration",
                        ) {
                            // Only if we do not have the repository locally do we fetch here.
                            // If we do have it, only fetch after receiving a ref announcement.
                            match self.storage.contains(id) {
                                Ok(true) => {
                                    // Do nothing.
                                }
                                Ok(false) => {
                                    debug!(target: "service", "Missing tracked inventory {id}; initiating fetch..");
                                    self.fetch(*id, announcer, FETCH_TIMEOUT);
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
            // Process a peer inventory update announcement by (maybe) fetching.
            AnnouncementMessage::Refs(message) => {
                // We update inventories when receiving ref announcements, as these could come
                // from a new repository being initialized.
                if let Ok(result) =
                    self.routing
                        .insert([&message.rid], *announcer, message.timestamp)
                {
                    if let &[(_, InsertResult::SeedAdded)] = result.as_slice() {
                        self.emitter.emit(Event::SeedDiscovered {
                            rid: message.rid,
                            nid: *relayer,
                        });
                        info!(target: "service", "Routing table updated for {} with seed {announcer}", message.rid);
                    }
                }

                // Check if the announcer is in sync with our own refs, and if so emit an event.
                // This event is used for showing sync progress to users.
                match message.is_synced(&self.node_id(), &self.storage) {
                    Ok(synced) => {
                        if synced {
                            self.emitter.emit(Event::RefsSynced {
                                rid: message.rid,
                                remote: *announcer,
                            });
                        }
                    }
                    Err(e) => {
                        error!(target: "service", "Error checking refs announcement sync status: {e}");
                    }
                }

                // TODO: Buffer/throttle fetches.
                let repo_entry = self.tracking.repo_policy(&message.rid).expect(
                    "Service::handle_announcement: error accessing repo tracking configuration",
                );

                if repo_entry.policy == tracking::Policy::Track {
                    // Refs can be relayed by peers who don't have the data in storage,
                    // therefore we only check whether we are connected to the *announcer*,
                    // which is required by the protocol to only announce refs it has.
                    if self.sessions.is_connected(announcer) {
                        match self.should_fetch_refs_announcement(message, &repo_entry.scope) {
                            Ok(Some(refs)) => {
                                self.fetch_refs_at(message.rid, announcer, refs, FETCH_TIMEOUT)
                            }
                            Ok(None) => {}
                            Err(e) => {
                                error!(target: "service", "Failed to check refs announcement: {e}");
                                return Err(session::Error::Misbehavior);
                            }
                        }
                    } else {
                        trace!(
                            target: "service",
                            "Skipping fetch of {}, no sessions connected to {announcer}",
                            message.rid
                        );
                    }
                    return Ok(relay);
                } else {
                    debug!(
                        target: "service",
                        "Ignoring refs announcement from {announcer}: repository {} isn't tracked",
                        message.rid
                    );
                }
            }
            AnnouncementMessage::Node(
                ann @ NodeAnnouncement {
                    features,
                    addresses,
                    ..
                },
            ) => {
                // If this node isn't a seed, we're not interested in adding it
                // to our address book, but other nodes may be, so we relay the message anyway.
                if !features.has(Features::SEED) {
                    return Ok(relay);
                }

                match self.addresses.insert(
                    announcer,
                    *features,
                    ann.alias.clone(),
                    ann.work(),
                    timestamp,
                    addresses
                        .iter()
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

    /// A convenient method to check if we should fetch from a `RefsAnnouncement`
    /// with `scope`.
    fn should_fetch_refs_announcement(
        &self,
        message: &RefsAnnouncement,
        scope: &tracking::Scope,
    ) -> Result<Option<Vec<RefsAt>>, Error> {
        // First, check the freshness.
        if !message.is_fresh(&self.storage)? {
            debug!(target: "service", "All refs of {} are already in local storage", &message.rid);
            return Ok(None);
        }

        // Second, check the scope.
        match scope {
            tracking::Scope::All => Ok(Some(message.refs.clone().into())),
            tracking::Scope::Trusted => {
                match self.tracking.namespaces_for(&self.storage, &message.rid) {
                    Ok(Namespaces::All) => Ok(Some(message.refs.clone().into())),
                    Ok(Namespaces::Trusted(mut trusted)) => {
                        // Get the set of trusted nodes except self.
                        trusted.remove(&self.node_id());

                        // Check if there is at least one trusted ref.
                        Ok(Some(
                            message
                                .refs
                                .iter()
                                .filter(|refs_at| trusted.contains(&refs_at.remote))
                                .cloned()
                                .collect(),
                        ))
                    }
                    Err(NamespacesError::NoTrusted { rid }) => {
                        debug!(target: "service", "No trusted nodes to fetch {}", &rid);
                        Ok(None)
                    }
                    Err(e) => {
                        error!(target: "service", "Failed to obtain namespaces: {e}");
                        Err(e.into())
                    }
                }
            }
        }
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
                if self.handle_announcement(&relayer, &relayer_addr, &ann)? {
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
                    .gossip
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
            (session::State::Connected { ping, .. }, Message::Pong { zeroes }) => {
                if let session::PingState::AwaitingResponse(ponglen) = *ping {
                    if (ponglen as usize) == zeroes.len() {
                        *ping = session::PingState::Ok;
                    }
                }
            }
            (session::State::Attempted { .. } | session::State::Initial, msg) => {
                error!(target: "service", "Received {:?} from connecting peer {}", msg, peer.id);
            }
            (session::State::Disconnected { .. }, msg) => {
                debug!(target: "service", "Ignoring {:?} from disconnected peer {}", msg, peer.id);
            }
        }
        Ok(())
    }

    /// Set of initial messages to send to a peer.
    fn initial(&self, _link: Link) -> Vec<Message> {
        let filter = self.filter();
        let now = self.clock();
        let inventory = match self.storage.inventory() {
            Ok(i) => i,
            Err(e) => {
                // Other than crashing the node completely, there's nothing we can do
                // here besides returning an empty inventory and logging an error.
                error!(target: "service", "Error getting local inventory for initial messages: {e}");
                vec![]
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
        let since = match self.gossip.last() {
            Ok(Some(last)) => last - MAX_TIME_DELTA.as_millis() as Timestamp,
            Ok(None) => (*now - INITIAL_SUBSCRIBE_BACKLOG_DELTA).as_millis() as Timestamp,
            Err(e) => {
                error!(target: "service", "Error getting the lastest gossip message from storage: {e}");
                return vec![];
            }
        };

        debug!(target: "service", "Subscribing to messages since timestamp {since}..");

        vec![
            Message::node(self.node.clone(), &self.signer),
            Message::inventory(gossip::inventory(now.as_millis(), inventory), &self.signer),
            Message::subscribe(filter, since, Timestamp::MAX),
        ]
    }

    /// Update our routing table with our local node's inventory.
    fn sync_inventory(&mut self) -> Result<SyncedRouting, Error> {
        let inventory = self.storage.inventory()?;
        let result = self.sync_routing(&inventory, self.node_id(), self.time())?;

        Ok(result)
    }

    /// Process a peer inventory announcement by updating our routing table.
    /// This function expects the peer's full inventory, and prunes entries that are not in the
    /// given inventory.
    fn sync_routing(
        &mut self,
        inventory: &[Id],
        from: NodeId,
        timestamp: Timestamp,
    ) -> Result<SyncedRouting, Error> {
        let mut synced = SyncedRouting::default();
        let included: HashSet<&Id> = HashSet::from_iter(inventory);

        for (rid, result) in self.routing.insert(inventory, from, timestamp)? {
            match result {
                InsertResult::SeedAdded => {
                    info!(target: "service", "Routing table updated for {rid} with seed {from}");
                    self.emitter.emit(Event::SeedDiscovered { rid, nid: from });

                    if self.tracking.is_repo_tracked(&rid).expect(
                        "Service::process_inventory: error accessing tracking configuration",
                    ) {
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
        for rid in self.routing.get_resources(&from)?.into_iter() {
            if !included.contains(&rid) {
                if self.routing.remove(&rid, &from)? {
                    synced.removed.push(rid);
                    self.emitter.emit(Event::SeedDropped { rid, nid: from });
                }
            }
        }
        Ok(synced)
    }

    /// Announce local refs for given id.
    fn announce_refs(
        &mut self,
        rid: Id,
        remotes: impl IntoIterator<Item = NodeId>,
    ) -> Result<(), Error> {
        let repo = self.storage.repository(rid)?;
        let doc = repo.identity_doc()?;
        let peers = self.sessions.connected().map(|(_, p)| p);
        let timestamp = self.time();
        let mut refs = BoundedVec::<_, REF_REMOTE_LIMIT>::new();

        for remote_id in remotes.into_iter() {
            if refs.push(RefsAt::new(&repo, remote_id)?).is_err() {
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
            refs,
            timestamp,
        });
        let ann = msg.signed(&self.signer);

        self.outbox.broadcast(
            ann,
            peers.filter(|p| {
                // Only announce to peers who are allowed to view this repo.
                doc.is_visible_to(&p.id)
            }),
        );

        Ok(())
    }

    fn sync_and_announce(&mut self) {
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
        let persistent = self.config.is_persistent(&nid);

        if let Err(e) = self.addresses.attempted(&nid, &addr, self.time()) {
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

    fn seeds(&self, rid: &Id) -> Result<Seeds, Error> {
        let mut seeds = Seeds::new(self.rng.clone());

        // First build a list from peers that have synced our own refs, if any.
        // This step is skipped if we don't have the repository yet, or don't have
        // our own refs.
        if let Ok(repo) = self.storage.repository(*rid) {
            if let Ok(local) = RefsAt::new(&repo, self.node_id()) {
                for seed in self.addresses.seeds(rid)? {
                    let seed = seed?;
                    let state = self.sessions.get(&seed.nid).map(|s| s.state.clone());
                    let synced = if local.at == seed.synced_at.oid {
                        SyncStatus::Synced { at: seed.synced_at }
                    } else {
                        SyncStatus::OutOfSync {
                            local: local.at,
                            remote: seed.synced_at.oid,
                        }
                    };
                    seeds.insert(Seed::new(seed.nid, seed.addresses, state, Some(synced)));
                }
            }
        }

        // Then, add peers we know about but have no information about the sync status.
        // These peers have announced that they track the repository via an inventory
        // announcement, but we haven't received any ref announcements from them.
        for nid in self.routing.get(rid)? {
            if nid == self.node_id() {
                continue;
            }
            if seeds.contains(&nid) {
                // We already have a richer entry for this node.
                continue;
            }
            let addrs = self.addresses.addresses(&nid)?;
            let state = self.sessions.get(&nid).map(|s| s.state.clone());

            seeds.insert(Seed::new(nid, addrs, state, None));
        }
        Ok(seeds)
    }

    /// Return a new filter object, based on our tracking policy.
    fn filter(&self) -> Filter {
        if self.config.policy == tracking::Policy::Track {
            // TODO: Remove bits for blocked repos.
            Filter::default()
        } else {
            self.filter.clone()
        }
    }

    /// Get the current time.
    fn time(&self) -> Timestamp {
        self.clock.as_millis()
    }

    ////////////////////////////////////////////////////////////////////////////
    // Periodic tasks
    ////////////////////////////////////////////////////////////////////////////

    /// Announce our inventory to all connected peers.
    fn announce_inventory(&mut self, inventory: Vec<Id>) -> Result<(), storage::Error> {
        let time = self.time();
        let inv = Message::inventory(gossip::inventory(time, inventory), &self.signer);
        for (_, sess) in self.sessions.connected() {
            self.outbox.write(sess, inv.clone());
        }
        Ok(())
    }

    fn prune_routing_entries(&mut self, now: &LocalTime) -> Result<(), routing::Error> {
        let count = self.routing.len()?;
        if count <= self.config.limits.routing_max_size {
            return Ok(());
        }

        let delta = count - self.config.limits.routing_max_size;
        self.routing.prune(
            (*now - self.config.limits.routing_max_age).as_millis(),
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
            session.ping(&mut self.outbox).ok();
        }
    }

    /// Get a list of peers available to connect to.
    fn available_peers(&mut self) -> HashMap<NodeId, Vec<KnownAddress>> {
        match self.addresses.entries() {
            Ok(entries) => {
                // Nb. we don't want to connect to any peers that already have a session with us,
                // even if it's in a disconnected state. Those sessions are re-attempted automatically.
                entries
                    .filter(|(_, ka)| !ka.banned)
                    .filter(|(nid, _)| !self.sessions.contains_key(nid))
                    .filter(|(nid, _)| nid != &self.node_id())
                    .fold(HashMap::new(), |mut acc, (nid, addr)| {
                        acc.entry(nid).or_insert_with(Vec::new).push(addr);
                        acc
                    })
            }
            Err(e) => {
                error!(target: "service", "Unable to lookup available peers in address book: {e}");
                HashMap::new()
            }
        }
    }

    /// Fetch all repositories that are tracked but missing from our inventory.
    fn fetch_missing_inventory(&mut self) -> Result<(), Error> {
        let inventory = self.storage().inventory()?;
        let missing = self
            .tracking
            .repo_policies()?
            .filter_map(|t| (t.policy == tracking::Policy::Track).then_some(t.id))
            .filter(|rid| !inventory.contains(rid));

        for rid in missing {
            match self.seeds(&rid) {
                Ok(seeds) => {
                    if let Some(connected) = NonEmpty::from_vec(seeds.connected().collect()) {
                        for seed in connected {
                            self.fetch(rid, &seed.nid, FETCH_TIMEOUT);
                        }
                    } else {
                        // TODO: We should make sure that this fetch is retried later, either
                        // when we connect to a seed, or when we discover a new seed.
                        // Since new connections and routing table updates are both conditions for
                        // fetching, we should trigger fetches when those conditions appear.
                        // Another way to handle this would be to update our database, saying
                        // that we're trying to fetch a certain repo. We would then just
                        // iterate over those entries in the above circumstances. This is
                        // merely an optimization though, we can also iterate over all tracked
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

        for (id, ka) in self
            .available_peers()
            .into_iter()
            .filter_map(|(nid, kas)| {
                kas.into_iter()
                    .find(|ka| match (ka.last_success, ka.last_attempt) {
                        // If we succeeded the last time we tried, this is a good address.
                        // TODO: This will always be hit after a success, and never re-attempted after
                        //       the first failed attempt.
                        (Some(success), attempt) => success >= attempt.unwrap_or_default(),
                        // If we haven't succeeded yet, and we waited long enough, we can try this address.
                        (None, Some(attempt)) => now - attempt >= CONNECTION_RETRY_DELTA,
                        // If we've never tried this address, it's worth a try.
                        (None, None) => true,
                    })
                    .map(|ka| (nid, ka))
            })
            .take(wanted)
        {
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
    fn get(&self, rid: Id) -> Result<Option<Doc<Verified>>, RepositoryError>;
    /// Get the clock.
    fn clock(&self) -> &LocalTime;
    /// Get the clock mutably.
    fn clock_mut(&mut self) -> &mut LocalTime;
    /// Get service configuration.
    fn config(&self) -> &Config;
}

impl<R, A, S, G> ServiceState for Service<R, A, S, G>
where
    R: routing::Store,
    G: Signer,
    S: ReadStorage,
{
    fn nid(&self) -> &NodeId {
        self.signer.public_key()
    }

    fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    fn get(&self, rid: Id) -> Result<Option<Doc<Verified>>, RepositoryError> {
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

    // TODO: These aren't quite correct, since dial errors *can* be transient, eg.
    // temporary DNS issue.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Dial(_) => false,
            Self::Connection(_) => true,
            Self::Command => false,
            Self::Fetch(_) => true,
            Self::Session(err) => err.is_transient(),
        }
    }
}

impl fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dial(err) => write!(f, "{err}"),
            Self::Connection(err) => write!(f, "{err}"),
            Self::Command => write!(f, "command"),
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
