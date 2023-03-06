#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::collapsible_if)]
pub mod config;
pub mod filter;
pub mod message;
pub mod reactor;
pub mod routing;
pub mod session;
pub mod tracking;

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::{fmt, net, str};

use crossbeam_channel as chan;
use fastrand::Rng;
use localtime::{LocalDuration, LocalTime};
use log::*;

use crate::address;
use crate::address::AddressBook;
use crate::clock::Timestamp;
use crate::crypto;
use crate::crypto::{Signer, Verified};
use crate::identity::IdentityError;
use crate::identity::{Doc, Id};
use crate::node;
use crate::node::{Address, Features, FetchResult, Seed, Seeds};
use crate::prelude::*;
use crate::service::message::{Announcement, AnnouncementMessage, Ping};
use crate::service::message::{NodeAnnouncement, RefsAnnouncement};
use crate::storage;
use crate::storage::{Inventory, ReadRepository, RefUpdate, WriteStorage};
use crate::storage::{Namespaces, ReadStorage};
use crate::worker::FetchError;
use crate::Link;

pub use crate::node::NodeId;
pub use crate::service::config::{Config, Network};
pub use crate::service::message::{Message, ZeroBytes};
pub use crate::service::reactor::Fetch;
pub use crate::service::session::Session;

use self::gossip::Gossip;
use self::message::InventoryAnnouncement;
use self::reactor::Reactor;

/// Target number of peers to maintain connections to.
pub const TARGET_OUTBOUND_PEERS: usize = 8;
/// How often to run the "idle" task.
pub const IDLE_INTERVAL: LocalDuration = LocalDuration::from_secs(30);
/// How often to run the "announce" task.
pub const ANNOUNCE_INTERVAL: LocalDuration = LocalDuration::from_secs(30);
/// How often to run the "sync" task.
pub const SYNC_INTERVAL: LocalDuration = LocalDuration::from_secs(60);
/// How often to run the "prune" task.
pub const PRUNE_INTERVAL: LocalDuration = LocalDuration::from_mins(30);
/// Duration to wait on an unresponsive peer before dropping its connection.
pub const STALE_CONNECTION_TIMEOUT: LocalDuration = LocalDuration::from_secs(60);
/// How much time should pass after a peer was last active for a *ping* to be sent.
pub const KEEP_ALIVE_DELTA: LocalDuration = LocalDuration::from_secs(30);
/// Maximum time difference between the local time, and an announcement timestamp.
pub const MAX_TIME_DELTA: LocalDuration = LocalDuration::from_mins(60);
/// Maximum attempts to connect to a peer before we give up.
pub const MAX_CONNECTION_ATTEMPTS: usize = 3;
/// How far back from the present time should we request gossip messages when connecting to a peer.
pub const SUBSCRIBE_BACKLOG_DELTA: LocalDuration = LocalDuration::from_mins(60);

/// Maximum external address limit imposed by message size limits.
pub use message::ADDRESS_LIMIT;
/// Maximum inventory limit imposed by message size limits.
pub use message::INVENTORY_LIMIT;
/// Maximum number of project git references imposed by message size limits.
pub use message::REF_REMOTE_LIMIT;

/// A service event.
#[derive(Debug, Clone)]
pub enum Event {
    RefsFetched {
        remote: NodeId,
        rid: Id,
        updated: Vec<RefUpdate>,
    },
}

/// General service error.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Routing(#[from] routing::Error),
    #[error(transparent)]
    Tracking(#[from] tracking::Error),
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
    Connect(NodeId, Address),
    /// Lookup seeds for the given repository in the routing table.
    Seeds(Id, chan::Sender<Seeds>),
    /// Fetch the given repository from the network.
    Fetch(Id, NodeId, chan::Sender<FetchResult>),
    /// Track the given repository.
    TrackRepo(Id, chan::Sender<bool>),
    /// Untrack the given repository.
    UntrackRepo(Id, chan::Sender<bool>),
    /// Track the given node.
    TrackNode(NodeId, Option<String>, chan::Sender<bool>),
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
            Self::Connect(id, addr) => write!(f, "Connect({id}, {addr})"),
            Self::Seeds(id, _) => write!(f, "Seeds({id})"),
            Self::Fetch(id, node, _) => write!(f, "Fetch({id}, {node})"),
            Self::TrackRepo(id, _) => write!(f, "TrackRepo({id})"),
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
    tracking: tracking::Config,
    /// State relating to gossip.
    gossip: Gossip,
    /// Peer sessions, currently or recently connected.
    sessions: Sessions,
    /// Keeps track of node states.
    nodes: BTreeMap<NodeId, Node>,
    /// Clock. Tells the time.
    clock: LocalTime,
    /// Interface to the I/O reactor.
    reactor: Reactor,
    /// Source of entropy.
    rng: Rng,
    /// Whether our local inventory no long represents what we have announced to the network.
    out_of_sync: bool,
    /// Fetch requests initiated by user, which are waiting for results.
    fetch_reqs: HashMap<Id, chan::Sender<FetchResult>>,
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
    S: WriteStorage + 'static,
    G: Signer,
{
    pub fn new(
        config: Config,
        clock: LocalTime,
        routing: R,
        storage: S,
        addresses: A,
        tracking: tracking::Config,
        signer: G,
        rng: Rng,
    ) -> Self {
        let sessions = Sessions::new(rng.clone());

        Self {
            config,
            storage,
            addresses,
            tracking,
            signer,
            rng,
            clock,
            routing,
            gossip: Gossip::default(),
            // FIXME: This should be loaded from the address store.
            nodes: BTreeMap::new(),
            reactor: Reactor::default(),
            sessions,
            out_of_sync: false,
            fetch_reqs: HashMap::new(),
            filter: Filter::empty(),
            last_idle: LocalTime::default(),
            last_sync: LocalTime::default(),
            last_prune: LocalTime::default(),
            last_announce: LocalTime::default(),
            start_time: LocalTime::default(),
        }
    }

    /// Return the next i/o action to execute.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<reactor::Io> {
        self.reactor.next()
    }

    /// Track a repository.
    /// Returns whether or not the tracking policy was updated.
    pub fn track_repo(&mut self, id: &Id, scope: tracking::Scope) -> Result<bool, tracking::Error> {
        self.out_of_sync = self.tracking.track_repo(id, scope)?;
        self.filter.insert(id);

        Ok(self.out_of_sync)
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
                .repo_entries()?
                .filter(|(_, _, policy)| *policy == tracking::Policy::Track)
                .map(|(e, _, _)| e),
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

    /// Get the storage instance.
    pub fn storage(&self) -> &S {
        &self.storage
    }

    /// Get the mutable storage instance.
    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    /// Get the tracking policy.
    pub fn tracking(&self) -> &tracking::Config {
        &self.tracking
    }

    /// Get the local signer.
    pub fn signer(&self) -> &G {
        &self.signer
    }

    /// Get I/O reactor.
    pub fn reactor(&mut self) -> &mut Reactor {
        &mut self.reactor
    }

    /// Lookup a project, both locally and in the routing table.
    pub fn lookup(&self, id: Id) -> Result<Lookup, LookupError> {
        let remote = self.routing.get(&id)?.iter().cloned().collect();

        Ok(Lookup {
            local: self.storage.get(&self.node_id(), id)?,
            remote,
        })
    }

    pub fn initialize(&mut self, time: LocalTime) -> Result<(), Error> {
        debug!(target: "service", "Init @{}", time.as_millis());

        self.start_time = time;

        // Connect to configured peers.
        let addrs = self.config.connect.clone();
        for (id, addr) in addrs {
            self.connect(id, addr);
        }
        // Ensure that our inventory is recorded in our routing table, and we are tracking
        // all of it. It can happen that inventory is not properly tracked if for eg. the
        // user creates a new repository while the node is stopped.
        for rid in self.storage.inventory()? {
            self.routing.insert(rid, self.node_id(), time.as_millis())?;

            if self
                .track_repo(&rid, tracking::Scope::All)
                .expect("Service::command: error tracking repository")
            {
                info!(target: "service", "Tracking local repository {rid}");
            }
        }
        // Setup subscription filter for tracked repos.
        self.filter = Filter::new(
            self.tracking
                .repo_entries()?
                .filter(|(_, _, policy)| *policy == tracking::Policy::Track)
                .map(|(e, _, _)| e),
        );

        Ok(())
    }

    pub fn tick(&mut self, now: LocalTime) {
        trace!("Tick +{}", now - self.start_time);

        self.clock = now;
    }

    pub fn wake(&mut self) {
        let now = self.clock;

        trace!(target: "service", "Wake +{}", now - self.start_time);

        if now - self.last_idle >= IDLE_INTERVAL {
            debug!(target: "service", "Running 'idle' task...");

            self.keep_alive(&now);
            self.disconnect_unresponsive_peers(&now);
            self.maintain_connections();
            self.reactor.wakeup(IDLE_INTERVAL);
            self.last_idle = now;
        }
        if now - self.last_sync >= SYNC_INTERVAL {
            debug!(target: "service", "Running 'sync' task...");

            // TODO: What do we do here?
            self.reactor.wakeup(SYNC_INTERVAL);
            self.last_sync = now;
        }
        if now - self.last_announce >= ANNOUNCE_INTERVAL {
            if self.out_of_sync {
                if let Err(err) = self
                    .storage
                    .inventory()
                    .and_then(|i| self.announce_inventory(i))
                {
                    error!("Error announcing inventory: {}", err);
                }
            }
            self.reactor.wakeup(ANNOUNCE_INTERVAL);
            self.last_announce = now;
        }
        if now - self.last_prune >= PRUNE_INTERVAL {
            debug!(target: "service", "Running 'prune' task...");

            if let Err(err) = self.prune_routing_entries(&now) {
                error!("Error pruning routing entries: {}", err);
            }
            self.reactor.wakeup(PRUNE_INTERVAL);
            self.last_prune = now;
        }
    }

    pub fn command(&mut self, cmd: Command) {
        debug!(target: "service", "Received command {:?}", cmd);

        match cmd {
            Command::Connect(id, addr) => {
                self.connect(id, addr);
            }
            Command::Seeds(rid, resp) => {
                #[derive(Default)]
                pub struct Stats {
                    connected: usize,
                    disconnected: usize,
                    fetching: usize,
                }

                let (stats, seeds) = match self.routing.get(&rid) {
                    Ok(seeds) => seeds.into_iter().fold(
                        (Stats::default(), Seeds::default()),
                        |(mut stats, mut seeds), node| {
                            if node != self.node_id() {
                                if self.sessions.is_fetching(&node) {
                                    seeds.insert(Seed::Fetching(node));
                                    stats.fetching += 1;
                                } else if self.sessions.is_connected(&node) {
                                    seeds.insert(Seed::Connected(node));
                                    stats.connected += 1;
                                } else if self.sessions.is_disconnected(&node) {
                                    seeds.insert(Seed::Disconnected(node));
                                    stats.connected += 1;
                                }
                            }

                            (stats, seeds)
                        },
                    ),
                    Err(err) => {
                        error!(target: "service", "Error reading routing table for {rid}: {err}");
                        drop(resp);

                        return;
                    }
                };
                debug!(
                    target: "service",
                    "Found {} connected seed(s), {} disconnected seed(s), and {} fetching seed(s) for {}",
                    stats.connected, stats.disconnected, stats.fetching, rid
                );
                resp.send(seeds).ok();
            }
            Command::Fetch(rid, seed, resp) => {
                // TODO: Establish connections to unconnected seeds, and retry.
                // TODO: Fetch requests should be queued and re-checked to see if they can
                //       be fulfilled everytime a new node connects.
                self.fetch_reqs.insert(rid, resp);
                self.fetch(rid, &seed);
            }
            Command::TrackRepo(rid, resp) => {
                let tracked = self
                    .track_repo(&rid, tracking::Scope::All)
                    .expect("Service::command: error tracking repository");
                // TODO: Try to fetch project if we weren't tracking it before.
                resp.send(tracked).ok();

                // Let all our peers know that we're interested in this repo from now on.
                self.reactor.broadcast(
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
                if let Err(err) = self.announce_refs(id, Namespaces::One(self.node_id())) {
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
                let updated = self
                    .sync_inventory()
                    .expect("Service::command: error syncing inventory");
                resp.send(!updated.is_empty()).ok();
            }
            Command::QueryState(query, sender) => {
                sender.send(query(self)).ok();
            }
        }
    }

    pub fn fetch(&mut self, rid: Id, from: &NodeId) {
        let Some(session) = self.sessions.get_mut(from) else {
            error!(target: "service", "Session {from} does not exist; cannot initiate fetch");
            return;
        };
        debug_assert!(session.is_connected());

        let seed = session.id;

        match session.fetch(rid) {
            session::FetchResult::Ready(fetch) => {
                debug!(target: "service", "Fetch initiated for {rid} with {seed}..");

                self.reactor.write(session, fetch);
                session.to_requesting(rid);
            }
            session::FetchResult::AlreadyFetching(other) => {
                if other == rid {
                    debug!(target: "service", "Ignoring redundant attempt to fetch {rid} from {from}");
                } else {
                    // TODO: If we can't fetch, it's because we're already fetching from
                    // this peer. So we need to queue the request, or find another peer.
                    error!(
                        target: "service",
                        "Dropping fetch for {rid} from {from}: another fetch is ongoing"
                    );
                }
            }
            session::FetchResult::NotConnected => {
                error!(target: "service", "Unable to fetch {rid} from peer {seed}: peer is not connected");
            }
        }
    }

    pub fn fetched(&mut self, fetch: Fetch, result: Result<Vec<RefUpdate>, FetchError>) {
        let remote = fetch.remote;
        let rid = fetch.rid;
        let initiated = fetch.initiated;

        if initiated {
            let result = match result {
                Ok(updated) => {
                    log::debug!(target: "service", "Fetched {rid} from {remote}");

                    self.reactor.event(Event::RefsFetched {
                        remote,
                        rid,
                        updated: updated.clone(),
                    });
                    FetchResult::Success { updated }
                }
                Err(err) => {
                    let reason = err.to_string();
                    error!(target: "service", "Fetch failed for {rid} from {remote}: {reason}");

                    // For now, we only disconnect the remote in case of timeout. In the future,
                    // there may be other reasons to disconnect.
                    if err.is_timeout() {
                        self.reactor
                            .disconnect(remote, DisconnectReason::Fetch(err));
                    }
                    FetchResult::Failed { reason }
                }
            };

            if let Some(results) = self.fetch_reqs.get(&rid) {
                log::debug!(target: "service", "Found existing fetch request, sending result..");

                if results.send(result).is_err() {
                    log::error!(target: "service", "Error sending fetch result for {rid}..");
                    // FIXME: We should remove the channel even on success, once all seeds
                    // were fetched from. Otherwise an organic fetch will try to send on the
                    // channel.
                    self.fetch_reqs.remove(&rid);
                } else {
                    log::debug!(target: "service", "Sent fetch result for {rid}..");
                }
            } else {
                log::debug!(target: "service", "No fetch requests found for {rid}..");

                // We only announce refs here when the fetch wasn't user-requested. This is
                // because the user might want to announce his fork, once he has created one,
                // or may choose to not announce anything.
                if let Err(e) = self.announce_refs(rid, fetch.namespaces) {
                    error!(target: "service", "Failed to announce new refs: {e}");
                }
            }
            // TODO: Since this fetch could be either a full clone or simply a ref update, we need
            // to either announce new inventory, or new refs. Right now, we announce both in some
            // cases.

            // Announce the newly fetched repository to the network, if necessary.
            match self.sync_inventory() {
                Ok(updated) => {
                    if !updated.is_empty() {
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

        if let Some(session) = self.sessions.get_mut(&remote) {
            // Transition session back to gossip protocol.
            session.to_gossip();
            // Drain any messages in the session's outbox, which might have accumulated during the
            // fetch, and send them to the peer.
            self.reactor.drain(session);
        } else {
            log::debug!(target: "service", "Session not found for {remote}");
        }
    }

    pub fn accepted(&mut self, _addr: net::SocketAddr) {
        // Inbound connection attempt.
    }

    pub fn attempted(&mut self, id: NodeId, addr: &Address) {
        debug!(target: "service", "Attempted connection to {id} ({addr})");

        let persistent = self.config.is_persistent(&id);
        self.sessions
            .entry(id)
            .and_modify(|sess| sess.to_connecting())
            .or_insert_with(|| Session::connecting(id, persistent, self.rng.clone()));
    }

    pub fn connected(&mut self, remote: NodeId, link: Link) {
        info!(target: "service", "Connected to {} ({:?})", remote, link);

        let msgs = self.initial(link);

        if link.is_outbound() {
            if let Some(peer) = self.sessions.get_mut(&remote) {
                peer.to_connected(self.clock);
                self.reactor.write_all(peer, msgs);
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
                    let peer = e.insert(Session::connected(
                        remote,
                        Link::Inbound,
                        self.config.is_persistent(&remote),
                        self.rng.clone(),
                        self.clock,
                    ));
                    self.reactor.write_all(peer, msgs);
                }
            }
        }
    }

    pub fn disconnected(&mut self, remote: NodeId, reason: &DisconnectReason) {
        let since = self.local_time();

        debug!(target: "service", "Disconnected from {} ({})", remote, reason);

        if let Some(session) = self.sessions.get_mut(&remote) {
            session.to_disconnected(since);

            // Attempt to re-connect to persistent peers.
            if let Some(address) = self.config.peer(&remote) {
                if session.attempts() < MAX_CONNECTION_ATTEMPTS {
                    if reason.is_dial_err() {
                        return;
                    }
                    if !reason.is_transient() {
                        return;
                    }
                    // TODO: Eventually we want a delay before attempting a reconnection,
                    // with exponential back-off.
                    debug!(target: "service",
                        "Reconnecting to {} (attempts={})...",
                        remote,
                        session.attempts()
                    );

                    // TODO: Try to reconnect only if the peer was attempted. A disconnect without
                    // even a successful attempt means that we're unlikely to be able to reconnect.

                    self.connect(remote, address.clone());
                }
            } else {
                self.sessions.remove(&remote);
                self.maintain_connections();
            }
        }
    }

    pub fn received_message(&mut self, remote: NodeId, message: Message) {
        match self.handle_message(&remote, message) {
            Err(session::Error::NotFound(id)) => {
                error!("Session not found for {id}");
            }
            Err(err) => {
                // If there's an error, stop processing messages from this peer.
                // However, we still relay messages returned up to this point.
                self.reactor
                    .disconnect(remote, DisconnectReason::Session(err));

                // FIXME: The peer should be set in a state such that we don't
                // process further messages.
            }
            Ok(()) => {}
        }
    }

    /// Handle an announcement message.
    ///
    /// Returns `true` if this announcement should be stored and relayed to connected peers,
    /// and `false` if it should not.
    pub fn handle_announcement(
        &mut self,
        relayer: &NodeId,
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
        let peer = self.nodes.entry(*announcer).or_insert_with(Node::default);

        // Don't allow messages from too far in the future.
        if timestamp.saturating_sub(now.as_millis()) > MAX_TIME_DELTA.as_millis() as u64 {
            return Err(session::Error::InvalidTimestamp(timestamp));
        }

        match message {
            AnnouncementMessage::Inventory(message) => {
                // Discard inventory messages we've already seen, otherwise update
                // out last seen time.
                if !peer.inventory_announced(timestamp) {
                    debug!(target: "service", "Ignoring stale inventory announcement from {announcer} (t={})", self.time());
                    return Ok(false);
                }

                match self.sync_routing(&message.inventory, *announcer, message.timestamp) {
                    Ok(updated) => {
                        if updated.is_empty() {
                            return Ok(false);
                        }
                    }
                    Err(e) => {
                        error!("Error processing inventory from {}: {}", announcer, e);
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
                            if let Ok(true) = self.storage.contains(id) {
                                // Do nothing.
                            } else {
                                // We may hit this branch due to an error returned by storage.
                                // We attempt to fetch in case of error because it's likely
                                // the repository was corrupted, and fetching will fix it.
                                self.fetch(*id, announcer);
                            }
                        }
                    }
                }

                return Ok(relay);
            }
            // Process a peer inventory update announcement by (maybe) fetching.
            AnnouncementMessage::Refs(message) => {
                for (remote_id, theirs) in message.refs.iter() {
                    if theirs.verify(remote_id).is_err() {
                        warn!(target: "service", "Peer {relayer} relayed refs announcement with invalid signature for {remote_id}");
                        return Err(session::Error::Misbehavior);
                    }
                }

                // We update inventories when receiving ref announcements, as these could come
                // from a new repository being initialized.
                if let Ok(updated) = self
                    .routing
                    .insert(message.rid, *relayer, message.timestamp)
                {
                    if updated {
                        info!(target: "service", "Routing table updated for {} with seed {relayer}", message.rid);
                    }
                }
                // TODO: Buffer/throttle fetches.
                if self
                    .tracking
                    .is_repo_tracked(&message.rid)
                    .expect("Service::handle_announcement: error accessing tracking configuration")
                {
                    // Discard announcement messages we've already seen, otherwise update
                    // our last seen time.
                    if !peer.refs_announced(message.rid, timestamp) {
                        debug!(target: "service", "Ignoring stale refs announcement from {announcer}");
                        return Ok(false);
                    }

                    // Refs can be relayed by peers who don't have the data in storage,
                    // therefore we only check whether we are connected to the *announcer*,
                    // which is required by the protocol to only announce refs it has.
                    if self.sessions.is_connected(announcer) {
                        match message.is_fresh(&self.storage) {
                            Ok(is_fresh) => {
                                if is_fresh {
                                    // TODO: Only fetch if the refs announced are for peers we're tracking.
                                    self.fetch(message.rid, announcer);
                                }
                            }
                            Err(e) => {
                                error!(target: "service", "Failed to check ref announcement freshness: {e}");
                            }
                        }
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
                    alias,
                    addresses,
                    ..
                },
            ) => {
                // Discard node messages we've already seen, otherwise update
                // our last seen time.
                if !peer.node_announced(timestamp) {
                    debug!(target: "service", "Ignoring stale node announcement from {announcer}");
                    return Ok(false);
                }

                if !ann.validate() {
                    warn!(target: "service", "Dropping node announcement from {announcer}: invalid proof-of-work");
                    return Ok(false);
                }

                let alias = match str::from_utf8(alias) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(target: "service", "Dropping node announcement from {announcer}: invalid alias: {e}");
                        return Ok(false);
                    }
                };

                // If this node isn't a seed, we're not interested in adding it
                // to our address book, but other nodes may be, so we relay the message anyway.
                if !features.has(Features::SEED) {
                    return Ok(relay);
                }

                match self.addresses.insert(
                    announcer,
                    *features,
                    alias,
                    timestamp,
                    addresses
                        .iter()
                        .map(|a| address::KnownAddress::new(a.clone(), address::Source::Peer)),
                ) {
                    Ok(updated) => {
                        // Only relay if we received new information.
                        if updated {
                            debug!(target: "service",
                                "Address store entry for node {announcer} updated at {timestamp}"
                            );
                            return Ok(relay);
                        }
                    }
                    Err(err) => {
                        // An error here is due to a fault in our address store.
                        error!("Error processing node announcement from {announcer}: {err}");
                    }
                }
            }
        }
        Ok(false)
    }

    pub fn handle_message(
        &mut self,
        remote: &NodeId,
        message: Message,
    ) -> Result<(), session::Error> {
        let Some(peer) = self.sessions.get_mut(remote) else {
            return Err(session::Error::NotFound(*remote));
        };
        peer.last_active = self.clock;

        debug!(target: "service", "Received message {:?} from {}", &message, peer.id);

        match (&mut peer.state, message) {
            (
                session::State::Connected {
                    protocol: session::Protocol::Fetch { .. },
                    ..
                },
                _,
            ) => {
                // This should never happen if the service is properly configured, since all
                // incoming data is sent directly to the Git worker.
                log::error!(target: "service", "Received gossip message from {remote} during git fetch");

                return Err(session::Error::Misbehavior);
            }
            // Process a peer announcement.
            (session::State::Connected { .. }, Message::Announcement(ann)) => {
                let relayer = peer.id;

                // Returning true here means that the message should be relayed.
                if self.handle_announcement(&relayer, &ann)? {
                    self.gossip.received(ann.clone(), ann.message.timestamp());

                    // Choose peers we should relay this message to.
                    // 1. Don't relay to the peer who sent us this message.
                    // 2. Don't relay to the peer who signed this announcement.
                    let relay_to = self
                        .sessions
                        .connected()
                        .filter(|(id, _)| *id != remote && *id != &ann.node);

                    self.reactor.relay(ann.clone(), relay_to.map(|(_, p)| p));

                    return Ok(());
                }
            }
            (session::State::Connected { .. }, Message::Subscribe(subscribe)) => {
                for ann in self
                    .gossip
                    // Filter announcements by interest.
                    .filtered(&subscribe.filter, subscribe.since, subscribe.until)
                    // Don't send announcements authored by the remote, back to the remote.
                    .filter(|ann| &ann.node != remote)
                {
                    self.reactor.write(peer, ann.into());
                }
                peer.subscribe = Some(subscribe);
            }
            (session::State::Connected { .. }, Message::Ping(Ping { ponglen, .. })) => {
                // Ignore pings which ask for too much data.
                if ponglen > Ping::MAX_PONG_ZEROES {
                    return Ok(());
                }
                self.reactor.write(
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
            (session::State::Connected { .. }, Message::Fetch { rid }) => {
                debug!(target: "service", "Fetch requested for {rid} from {remote}..");

                // TODO: Check that we have the repo first?

                // Accept the request and instruct the transport to handover the socket to the worker.
                self.reactor.write(peer, Message::FetchOk { rid });
                self.reactor.fetch(peer, rid, Namespaces::default(), false);
            }
            (session::State::Connected { protocol, .. }, Message::FetchOk { rid }) => {
                if *protocol
                    != (session::Protocol::Gossip {
                        requested: Some(rid),
                    })
                {
                    // As long as we disconnect peers who don't respond to our fetch requests within
                    // the alloted time, this shouldn't happen by mistake.
                    error!(
                        "Received unexpected message `fetch-ok` from peer {}",
                        peer.id
                    );
                    return Err(session::Error::Misbehavior);
                }
                debug!(target: "service", "Fetch accepted for {rid} from {remote}..");

                // Instruct the transport to handover the socket to the worker.
                self.reactor.fetch(peer, rid, Namespaces::default(), true);
            }
            (session::State::Connecting { .. }, msg) => {
                error!("Received {:?} from connecting peer {}", msg, peer.id);
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

        // TODO: Only subscribe to outbound connections, otherwise we will consume too
        // much bandwidth.

        gossip::handshake(
            self.clock.as_millis(),
            &self.storage,
            &self.signer,
            filter,
            &self.config,
        )
    }

    /// Sync, and if needed, announce our local inventory.
    fn sync_inventory(&mut self) -> Result<Vec<Id>, Error> {
        let inventory = self.storage.inventory()?;
        let updated = self.sync_routing(&inventory, self.node_id(), self.time())?;

        Ok(updated)
    }

    /// Process a peer inventory announcement by updating our routing table.
    /// This function expects the peer's full inventory, and prunes entries that are not in the
    /// given inventory.
    fn sync_routing(
        &mut self,
        inventory: &[Id],
        from: NodeId,
        timestamp: Timestamp,
    ) -> Result<Vec<Id>, Error> {
        let mut updated = Vec::new();
        let mut included = HashSet::new();

        for proj_id in inventory {
            included.insert(proj_id);
            if self.routing.insert(*proj_id, from, timestamp)? {
                info!(target: "service", "Routing table updated for {proj_id} with seed {from}");

                if self
                    .tracking
                    .is_repo_tracked(proj_id)
                    .expect("Service::process_inventory: error accessing tracking configuration")
                {
                    // TODO: We should fetch here if we're already connected, case this seed has
                    // refs we don't have.
                }
                updated.push(*proj_id);
            }
        }
        for id in self.routing.get_resources(&from)?.into_iter() {
            if !included.contains(&id) {
                if self.routing.remove(&id, &from)? {
                    updated.push(id);
                }
            }
        }
        Ok(updated)
    }

    /// Announce local refs for given id.
    fn announce_refs(&mut self, rid: Id, namespaces: Namespaces) -> Result<(), storage::Error> {
        let repo = self.storage.repository(rid)?;
        let peers = self.sessions.connected().map(|(_, p)| p);
        let timestamp = self.time();
        let mut refs = BoundedVec::<_, REF_REMOTE_LIMIT>::new();

        match namespaces {
            Namespaces::All => {
                for (remote_id, remote) in repo.remotes()?.into_iter() {
                    if refs.push((remote_id, remote.refs.unverified())).is_err() {
                        warn!(
                            target: "service",
                            "refs announcement limit ({}) exceeded, peers will see only some of your repository references",
                            REF_REMOTE_LIMIT,
                        );
                        break;
                    }
                }
            }
            Namespaces::One(pk) => refs
                .push((pk, repo.remote(&pk)?.refs.unverified()))
                // SAFETY: `REF_REMOTE_LIMIT` is greater than 1, thus the bounded vec can hold at least
                // one remote.
                .unwrap(),
        }

        let msg = AnnouncementMessage::from(RefsAnnouncement {
            rid,
            refs,
            timestamp,
        });
        let ann = msg.signed(&self.signer);

        self.reactor.broadcast(ann, peers);

        Ok(())
    }

    fn connect(&mut self, node: NodeId, addr: Address) -> bool {
        if self.sessions.is_disconnected(&node) {
            self.reactor.connect(node, addr);
            return true;
        }
        log::warn!(target: "service", "Attempted connection to peer {node} which already has a session");

        false
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
            self.reactor.write(sess, inv.clone());
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
            .filter(|(_, session)| session.last_active < *now - STALE_CONNECTION_TIMEOUT);

        for (_, session) in stale {
            self.reactor.disconnect(
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
            session.ping(&mut self.reactor).ok();
        }
    }

    fn choose_addresses(&mut self) -> Vec<(NodeId, Address)> {
        let sessions = self
            .sessions
            .values()
            .filter(|s| s.is_connected() && s.link.is_outbound())
            .map(|s| (s.id, s))
            .collect::<HashMap<_, _>>();

        let wanted = TARGET_OUTBOUND_PEERS.saturating_sub(sessions.len());
        if wanted == 0 {
            return Vec::new();
        }

        self.addresses
            .entries()
            .unwrap()
            .filter(|(node_id, _)| !sessions.contains_key(node_id))
            .take(wanted)
            .map(|(n, s)| (n, s.addr))
            .collect()
    }

    fn maintain_connections(&mut self) {
        let addrs = self.choose_addresses();
        if addrs.is_empty() {
            debug!(target: "service", "No eligible peers available to connect to");
        }
        for (id, addr) in addrs {
            self.connect(id, addr.clone());
        }
    }
}

/// Gives read access to the service state.
pub trait ServiceState {
    /// Get the connected peers.
    fn sessions(&self) -> &Sessions;
    /// Get the current inventory.
    fn inventory(&self) -> Result<Inventory, storage::Error>;
    /// Get a repository from storage, using the local node's key.
    fn get(&self, proj: Id) -> Result<Option<Doc<Verified>>, IdentityError>;
    /// Get the clock.
    fn clock(&self) -> &LocalTime;
    /// Get the clock mutably.
    fn clock_mut(&mut self) -> &mut LocalTime;
    /// Get service configuration.
    fn config(&self) -> &Config;
    /// Get reference to routing table.
    fn routing(&self) -> &dyn routing::Store;
}

impl<R, A, S, G> ServiceState for Service<R, A, S, G>
where
    R: routing::Store,
    G: Signer,
    S: ReadStorage,
{
    fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    fn inventory(&self) -> Result<Inventory, storage::Error> {
        self.storage.inventory()
    }

    fn get(&self, proj: Id) -> Result<Option<Doc<Verified>>, IdentityError> {
        self.storage.get(&self.node_id(), proj)
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

    fn routing(&self) -> &dyn routing::Store {
        &self.routing
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
}

impl DisconnectReason {
    pub fn is_dial_err(&self) -> bool {
        matches!(self, Self::Dial(_))
    }

    pub fn is_connection_err(&self) -> bool {
        matches!(self, Self::Connection(_))
    }

    pub fn is_transient(&self) -> bool {
        match self {
            Self::Dial(_) => false,
            Self::Connection(_) => true,
            Self::Session(..) => false,
            Self::Fetch(_) => true,
        }
    }
}

impl fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dial(err) => write!(f, "{err}"),
            Self::Connection(err) => write!(f, "{err}"),
            Self::Session(err) => write!(f, "error: {err}"),
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
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Routing(#[from] routing::Error),
    #[error(transparent)]
    Identity(#[from] IdentityError),
}

/// Information on a peer, that we may or may not be connected to.
#[derive(Default, Debug)]
pub struct Node {
    /// Last ref announcements (per project).
    pub last_refs: HashMap<Id, Timestamp>,
    /// Last inventory announcement.
    pub last_inventory: Timestamp,
    /// Last node announcement.
    pub last_node: Timestamp,
}

impl Node {
    /// Process a refs announcement for the given node.
    /// Returns `true` if the timestamp was updated.
    pub fn refs_announced(&mut self, id: Id, t: Timestamp) -> bool {
        match self.last_refs.entry(id) {
            Entry::Vacant(e) => {
                e.insert(t);
                return true;
            }
            Entry::Occupied(mut e) => {
                let last = e.get_mut();

                if t > *last {
                    *last = t;
                    return true;
                }
            }
        }
        false
    }

    /// Process an inventory announcement for the given node.
    /// Returns `true` if the timestamp was updated.
    pub fn inventory_announced(&mut self, t: Timestamp) -> bool {
        if t > self.last_inventory {
            self.last_inventory = t;
            return true;
        }
        false
    }

    /// Process a node announcement for the given node.
    /// Returns `true` if the timestamp was updated.
    pub fn node_announced(&mut self, t: Timestamp) -> bool {
        if t > self.last_node {
            self.last_node = t;
            return true;
        }
        false
    }
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
        self.0.iter_mut().filter(move |(_, p)| p.is_connected())
    }

    /// Return whether this node has a fully established session.
    pub fn is_connected(&self, id: &NodeId) -> bool {
        self.0.get(id).map(|s| s.is_connected()).unwrap_or(false)
    }

    pub fn is_fetching(&self, id: &NodeId) -> bool {
        self.0.get(id).map(|s| s.is_fetching()).unwrap_or(false)
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

mod gossip {
    use super::*;
    use crate::service::filter::Filter;

    #[derive(Default, Debug)]
    pub struct Gossip {
        received: Vec<(Timestamp, Announcement)>,
    }

    impl Gossip {
        // TODO: Overwrite old messages from the same node or project.
        // TODO: Should "time" be this node's time, or the time inside the message?
        pub fn received(&mut self, ann: Announcement, time: Timestamp) {
            self.received.push((time, ann));
        }

        pub fn filtered<'a>(
            &'a self,
            filter: &'a Filter,
            start: Timestamp,
            end: Timestamp,
        ) -> impl Iterator<Item = Announcement> + '_ {
            self.received
                .iter()
                .filter(move |(t, _)| *t >= start && *t < end)
                .filter(move |(_, a)| a.matches(filter))
                .cloned()
                .map(|(_, ann)| ann)
        }
    }

    pub fn handshake<G: Signer, S: ReadStorage>(
        now: Timestamp,
        storage: &S,
        signer: &G,
        filter: Filter,
        config: &Config,
    ) -> Vec<Message> {
        let inventory = match storage.inventory() {
            Ok(i) => i,
            Err(e) => {
                error!("Error getting local inventory for handshake: {}", e);
                // Other than crashing the node completely, there's nothing we can do
                // here besides returning an empty inventory and logging an error.
                vec![]
            }
        };

        let mut msgs = vec![
            Message::inventory(gossip::inventory(now, inventory), signer),
            Message::subscribe(
                filter,
                now - SUBSCRIBE_BACKLOG_DELTA.as_millis() as u64,
                Timestamp::MAX,
            ),
        ];
        if let Some(m) = gossip::node(now, config) {
            msgs.push(Message::node(m, signer));
        };

        msgs
    }

    pub fn node(timestamp: Timestamp, config: &Config) -> Option<NodeAnnouncement> {
        let features = node::Features::SEED;
        let alias = config.alias();
        let addresses: BoundedVec<_, ADDRESS_LIMIT> = config
            .external_addresses
            .clone()
            .try_into()
            .expect("external addresses are within the limit");

        if addresses.is_empty() {
            return None;
        }

        Some(
            NodeAnnouncement {
                features,
                timestamp,
                alias,
                addresses,
                nonce: 0,
            }
            .solve(),
        )
    }

    pub fn inventory(timestamp: Timestamp, inventory: Vec<Id>) -> InventoryAnnouncement {
        type Inventory = BoundedVec<Id, INVENTORY_LIMIT>;

        if inventory.len() > Inventory::max() {
            error!(
                target: "service",
                "inventory announcement limit ({}) exceeded, other nodes will see only some of your projects",
                inventory.len()
            );
        }

        InventoryAnnouncement {
            inventory: BoundedVec::truncate(inventory),
            timestamp,
        }
    }
}
