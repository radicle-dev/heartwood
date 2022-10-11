pub mod config;
pub mod filter;
pub mod message;
pub mod peer;
pub mod reactor;
pub mod routing;

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::{fmt, net, net::IpAddr, str};

use crossbeam_channel as chan;
use fastrand::Rng;
use log::*;
use nakamoto::{LocalDuration, LocalTime};
use nakamoto_net as nakamoto;
use nakamoto_net::Link;
use nonempty::NonEmpty;
use radicle::node::Features;
use radicle::storage::ReadStorage;

use crate::address;
use crate::address::AddressBook;
use crate::clock::{RefClock, Timestamp};
use crate::crypto;
use crate::crypto::{Signer, Verified};
use crate::git;
use crate::git::Url;
use crate::identity::{Doc, Id};
use crate::node;
use crate::service::config::ProjectTracking;
use crate::service::message::{Address, Announcement, AnnouncementMessage};
use crate::service::message::{NodeAnnouncement, RefsAnnouncement};
use crate::service::peer::{SessionError, SessionState};
use crate::storage;
use crate::storage::{Inventory, ReadRepository, RefUpdate, WriteRepository, WriteStorage};

pub use crate::service::config::{Config, Network};
pub use crate::service::message::{Envelope, Message};
pub use crate::service::peer::Session;

use self::gossip::Gossip;
use self::message::InventoryAnnouncement;
use self::reactor::Reactor;

pub const DEFAULT_PORT: u16 = 8776;
pub const PROTOCOL_VERSION: u32 = 1;
pub const TARGET_OUTBOUND_PEERS: usize = 8;
pub const IDLE_INTERVAL: LocalDuration = LocalDuration::from_secs(30);
pub const ANNOUNCE_INTERVAL: LocalDuration = LocalDuration::from_secs(30);
pub const SYNC_INTERVAL: LocalDuration = LocalDuration::from_secs(60);
pub const PRUNE_INTERVAL: LocalDuration = LocalDuration::from_mins(30);
pub const MAX_CONNECTION_ATTEMPTS: usize = 3;
pub const MAX_TIME_DELTA: LocalDuration = LocalDuration::from_mins(60);

/// Network node identifier.
pub type NodeId = crypto::PublicKey;

/// A service event.
#[derive(Debug, Clone)]
pub enum Event {
    RefsFetched {
        from: Url,
        project: Id,
        updated: Vec<RefUpdate>,
    },
}

/// General service error.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Fetch(#[from] storage::FetchError),
    #[error(transparent)]
    Routing(#[from] routing::Error),
}

/// Error returned by [`Command::Fetch`].
#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Fetch(#[from] storage::FetchError),
}

/// Result of looking up seeds in our routing table.
#[derive(Debug)]
pub enum FetchLookup {
    /// Found seeds for the given project.
    Found {
        seeds: NonEmpty<net::SocketAddr>,
        results: chan::Receiver<FetchResult>,
    },
    /// Can't fetch because no seeds were found for this project.
    NotFound,
    /// Can't fetch because the project isn't tracked.
    NotTracking,
    /// Error trying to find seeds.
    Error(FetchError),
}

/// Result of a fetch request from a specific seed.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum FetchResult {
    /// Successful fetch from a seed.
    Fetched {
        from: net::SocketAddr,
        updated: Vec<RefUpdate>,
    },
    /// Error fetching the resource from a seed.
    Error {
        from: net::SocketAddr,
        error: FetchError,
    },
}

/// Function used to query internal service state.
pub type QueryState = dyn Fn(&dyn ServiceState) -> Result<(), CommandError> + Send + Sync;

/// Commands sent to the service by the operator.
pub enum Command {
    /// Announce repository references for given project id to peers.
    AnnounceRefs(Id),
    /// Connect to node with the given address.
    Connect(net::SocketAddr),
    /// Fetch the given project from the network.
    Fetch(Id, chan::Sender<FetchLookup>),
    /// Track the given project.
    Track(Id, chan::Sender<bool>),
    /// Untrack the given project.
    Untrack(Id, chan::Sender<bool>),
    /// Query the internal service state.
    QueryState(Arc<QueryState>, chan::Sender<Result<(), CommandError>>),
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnnounceRefs(id) => write!(f, "AnnounceRefs({})", id),
            Self::Connect(addr) => write!(f, "Connect({})", addr),
            Self::Fetch(id, _) => write!(f, "Fetch({})", id),
            Self::Track(id, _) => write!(f, "Track({})", id),
            Self::Untrack(id, _) => write!(f, "Untrack({})", id),
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
    /// State relating to gossip.
    gossip: Gossip,
    /// Peer sessions, currently or recently connected.
    sessions: Sessions,
    /// Keeps track of peer states.
    peers: BTreeMap<NodeId, Peer>,
    /// Clock. Tells the time.
    clock: RefClock,
    /// Interface to the I/O reactor.
    reactor: Reactor,
    /// Source of entropy.
    rng: Rng,
    /// Whether our local inventory no long represents what we have announced to the network.
    out_of_sync: bool,
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
        self.clock.local_time()
    }
}

impl<R, A, S, G> Service<R, A, S, G>
where
    R: routing::Store,
    A: address::Store,
    S: WriteStorage + 'static,
    G: crypto::Signer,
{
    pub fn new(
        config: Config,
        clock: RefClock,
        routing: R,
        storage: S,
        addresses: A,
        signer: G,
        rng: Rng,
    ) -> Self {
        let sessions = Sessions::new(rng.clone());
        let network = config.network;

        Self {
            config,
            storage,
            addresses,
            signer,
            rng,
            clock,
            routing,
            gossip: Gossip::default(),
            // FIXME: This should be loaded from the address store.
            peers: BTreeMap::new(),
            reactor: Reactor::new(network),
            sessions,
            out_of_sync: false,
            last_idle: LocalTime::default(),
            last_sync: LocalTime::default(),
            last_prune: LocalTime::default(),
            last_announce: LocalTime::default(),
            start_time: LocalTime::default(),
        }
    }

    pub fn seeds(&self, id: &Id) -> Vec<(NodeId, &Session)> {
        if let Ok(seeds) = self.routing.get(id) {
            seeds
                .into_iter()
                .filter_map(|id| self.sessions.by_id(&id).map(|p| (id, p)))
                .collect()
        } else {
            vec![]
        }
    }

    pub fn tracked(&self) -> Result<Vec<Id>, storage::Error> {
        let tracked = match &self.config.project_tracking {
            ProjectTracking::All { blocked } => self
                .storage
                .inventory()?
                .into_iter()
                .filter(|id| !blocked.contains(id))
                .collect(),

            ProjectTracking::Allowed(projs) => projs.iter().cloned().collect(),
        };

        Ok(tracked)
    }

    /// Track a project.
    /// Returns whether or not the tracking policy was updated.
    pub fn track(&mut self, id: Id) -> bool {
        self.out_of_sync = self.config.track(id);
        self.out_of_sync
    }

    /// Untrack a project.
    /// Returns whether or not the tracking policy was updated.
    /// Note that when untracking, we don't announce anything to the network. This is because by
    /// simply not announcing it anymore, it will eventually be pruned by nodes.
    pub fn untrack(&mut self, id: Id) -> bool {
        self.config.untrack(id)
    }

    /// Find the closest `n` peers by proximity in tracking graphs.
    /// Returns a sorted list from the closest peer to the furthest.
    /// Peers with more trackings in common score score higher.
    #[allow(unused)]
    pub fn closest_peers(&self, n: usize) -> Vec<NodeId> {
        todo!()
    }

    /// Get the storage instance.
    pub fn storage(&self) -> &S {
        &self.storage
    }

    /// Get the mutable storage instance.
    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
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

    pub fn initialize(&mut self, time: LocalTime) {
        trace!("Init {}", time.as_secs());

        self.start_time = time;

        // Connect to configured peers.
        let addrs = self.config.connect.clone();
        for addr in addrs {
            self.reactor.connect(addr);
        }
    }

    pub fn tick(&mut self, now: nakamoto::LocalTime) {
        trace!("Tick +{}", now - self.start_time);

        self.clock.set(now);
    }

    pub fn wake(&mut self) {
        let now = self.clock.local_time();

        trace!("Wake +{}", now - self.start_time);

        if now - self.last_idle >= IDLE_INTERVAL {
            debug!("Running 'idle' task...");

            self.maintain_connections();
            self.reactor.wakeup(IDLE_INTERVAL);
            self.last_idle = now;
        }
        if now - self.last_sync >= SYNC_INTERVAL {
            debug!("Running 'sync' task...");

            // TODO: What do we do here?
            self.reactor.wakeup(SYNC_INTERVAL);
            self.last_sync = now;
        }
        if now - self.last_announce >= ANNOUNCE_INTERVAL {
            if self.out_of_sync {
                if let Err(err) = self.announce_inventory() {
                    error!("Error announcing inventory: {}", err);
                }
            }
            self.reactor.wakeup(ANNOUNCE_INTERVAL);
            self.last_announce = now;
        }
        if now - self.last_prune >= PRUNE_INTERVAL {
            debug!("Running 'prune' task...");

            if let Err(err) = self.prune_routing_entries() {
                error!("Error pruning routing entries: {}", err);
            }
            self.reactor.wakeup(PRUNE_INTERVAL);
            self.last_prune = now;
        }
    }

    pub fn command(&mut self, cmd: Command) {
        debug!("Command {:?}", cmd);

        match cmd {
            Command::Connect(addr) => self.reactor.connect(addr),
            Command::Fetch(id, resp) => {
                if !self.config.is_tracking(&id) {
                    resp.send(FetchLookup::NotTracking).ok();
                    return;
                }

                let seeds = self.seeds(&id);
                let seeds = if let Some(seeds) = NonEmpty::from_vec(seeds) {
                    seeds
                } else {
                    log::error!("No seeds found for {}", id);
                    resp.send(FetchLookup::NotFound).ok();

                    return;
                };
                log::debug!("Found {} seeds for {}", seeds.len(), id);

                let mut repo = match self.storage.repository(id) {
                    Ok(repo) => repo,
                    Err(err) => {
                        log::error!("Error opening repo for {}: {}", id, err);
                        resp.send(FetchLookup::Error(err.into())).ok();

                        return;
                    }
                };

                let (results_, results) = chan::bounded(seeds.len());
                resp.send(FetchLookup::Found {
                    seeds: seeds.clone().map(|(_, peer)| peer.addr),
                    results,
                })
                .ok();

                // TODO: Limit the number of seeds we fetch from? Randomize?
                for (_, peer) in seeds {
                    match repo.fetch(&Url {
                        scheme: git::url::Scheme::Git,
                        host: Some(peer.addr.ip().to_string()),
                        port: Some(peer.addr.port()),
                        // TODO: Fix upstream crate so that it adds a `/` when needed.
                        path: format!("/{}", id).into(),
                        ..Url::default()
                    }) {
                        Ok(updated) => {
                            results_
                                .send(FetchResult::Fetched {
                                    from: peer.addr,
                                    updated,
                                })
                                .ok();
                        }
                        Err(err) => {
                            results_
                                .send(FetchResult::Error {
                                    from: peer.addr,
                                    error: err.into(),
                                })
                                .ok();
                        }
                    }
                }
            }
            Command::Track(id, resp) => {
                resp.send(self.track(id)).ok();
            }
            Command::Untrack(id, resp) => {
                resp.send(self.untrack(id)).ok();
            }
            Command::AnnounceRefs(id) => {
                if let Err(err) = self.announce_refs(id) {
                    error!("Error announcing refs: {}", err);
                }
            }
            Command::QueryState(query, sender) => {
                sender.send(query(self)).ok();
            }
        }
    }

    pub fn attempted(&mut self, addr: &std::net::SocketAddr) {
        let address = Address::from(*addr);
        let ip = addr.ip();
        let persistent = self.config.is_persistent(&address);
        let peer = self
            .sessions
            .entry(ip)
            .or_insert_with(|| Session::new(*addr, Link::Outbound, persistent));

        peer.attempted();
    }

    pub fn connected(
        &mut self,
        addr: std::net::SocketAddr,
        _local_addr: &std::net::SocketAddr,
        link: Link,
    ) {
        let ip = addr.ip();
        let address = addr.into();

        debug!("Connected to {} ({:?})", ip, link);

        // For outbound connections, we are the first to say "Hello".
        // For inbound connections, we wait for the remote to say "Hello" first.
        // TODO: How should we deal with multiple peers connecting from the same IP address?
        if link.is_outbound() {
            if let Some(peer) = self.sessions.get_mut(&ip) {
                if link.is_outbound() {
                    self.reactor.write_all(
                        addr,
                        gossip::handshake(
                            self.clock.timestamp(),
                            &self.storage,
                            &self.signer,
                            &self.config,
                        ),
                    );
                }
                peer.connected(link);
            }
        } else {
            self.sessions.insert(
                ip,
                Session::new(addr, Link::Inbound, self.config.is_persistent(&address)),
            );
        }
    }

    pub fn disconnected(
        &mut self,
        addr: &std::net::SocketAddr,
        reason: &nakamoto::DisconnectReason<DisconnectReason>,
    ) {
        let since = self.local_time();
        let address = Address::from(*addr);
        let ip = addr.ip();

        debug!("Disconnected from {} ({})", ip, reason);

        if let Some(peer) = self.sessions.get_mut(&ip) {
            peer.state = SessionState::Disconnected { since };

            // Attempt to re-connect to persistent peers.
            if self.config.is_persistent(&address) && peer.attempts() < MAX_CONNECTION_ATTEMPTS {
                if reason.is_dial_err() {
                    return;
                }
                if let nakamoto::DisconnectReason::Protocol(r) = reason {
                    if !r.is_transient() {
                        return;
                    }
                }
                // TODO: Eventually we want a delay before attempting a reconnection,
                // with exponential back-off.
                debug!("Reconnecting to {} (attempts={})...", ip, peer.attempts());

                // TODO: Try to reconnect only if the peer was attempted. A disconnect without
                // even a successful attempt means that we're unlikely to be able to reconnect.

                self.reactor.connect(*addr);
            } else {
                // TODO: Non-persistent peers should be removed from the
                // map here or at some later point.
            }
        }
    }

    pub fn received_message(&mut self, addr: &net::SocketAddr, envelope: Envelope) {
        match self.handle_message(addr, envelope) {
            Ok(relay) => {
                if let Some(ann) = relay {
                    let negotiated = self
                        .sessions
                        .negotiated()
                        .filter(|(ip, _)| **ip != addr.ip())
                        .map(|(_, p)| p);

                    self.reactor.relay(ann, negotiated.clone());
                }
            }
            Err(SessionError::NotFound(ip)) => {
                error!("Session not found for {}", ip);
            }
            Err(err) => {
                // If there's an error, stop processing messages from this peer.
                // However, we still relay messages returned up to this point.
                self.reactor.disconnect(*addr, DisconnectReason::Error(err));

                // FIXME: The peer should be set in a state such that we don'that
                // process further messages.
            }
        }
    }

    /// Handle an announcement message.
    ///
    /// Returns `true` if this announcement should be stored and relayed to connected peers,
    /// and `false` if it should not.
    pub fn handle_announcement(
        &mut self,
        session: &NodeId,
        git: &git::Url,
        announcement: &Announcement,
    ) -> Result<bool, peer::SessionError> {
        if !announcement.verify() {
            return Err(SessionError::Misbehavior);
        }
        let Announcement { node, message, .. } = announcement;
        let now = self.clock.local_time();
        let timestamp = message.timestamp();
        let relay = self.config.relay;

        // Don't allow messages from too far in the future.
        if timestamp.saturating_sub(now.as_secs()) > MAX_TIME_DELTA.as_secs() {
            return Err(SessionError::InvalidTimestamp(timestamp));
        }

        match message {
            AnnouncementMessage::Inventory(message) => {
                // Discard inventory messages we've already seen, otherwise update
                // out last seen time.
                let peer = self.peers.entry(*node).or_insert_with(Peer::default);
                if timestamp > peer.last_message {
                    peer.last_message = timestamp;
                } else {
                    debug!("Ignoring stale announcement from {node}");
                    return Ok(false);
                }

                if let Err(err) = self.process_inventory(&message.inventory, *node, git) {
                    error!("Error processing inventory from {}: {}", node, err);

                    if let Error::Fetch(storage::FetchError::Verify(err)) = err {
                        // Disconnect the peer if it is the signer of this message.
                        if node == session {
                            return Err(peer::SessionError::VerificationFailed(err));
                        }
                    }
                    // There's not much we can do if the peer sending us this message isn't the
                    // origin of it.
                    return Ok(false);
                }
                return Ok(relay);
            }
            // Process a peer inventory update announcement by (maybe) fetching.
            AnnouncementMessage::Refs(message) => {
                // TODO: Buffer/throttle fetches.
                // TODO: Check that we're tracking this user as well.
                // FIXME: Discard old messages.
                if self.config.is_tracking(&message.id) {
                    // TODO: Check refs to see if we should try to fetch or not.
                    // FIXME: This code is wrong: we shouldn't be fetching from the connected peer,
                    // we should fetch from the origin.
                    let updated = self.storage.fetch(message.id, git).unwrap();
                    let is_updated = !updated.is_empty();

                    self.reactor.event(Event::RefsFetched {
                        from: git.clone(),
                        project: message.id,
                        updated,
                    });

                    if is_updated {
                        return Ok(relay);
                    }
                }
            }
            AnnouncementMessage::Node(NodeAnnouncement {
                features,
                alias,
                addresses,
                timestamp,
            }) => {
                // FIXME: Discard old messages.
                // If this node isn't a seed, we're not interested in adding it
                // to our address book, but other nodes may be, so we relay the message anyway.
                if !features.has(Features::SEED) {
                    return Ok(relay);
                }

                let alias = match str::from_utf8(alias) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("Dropping node announcement from {node}: invalid alias: {e}");
                        return Ok(false);
                    }
                };

                match self.addresses.insert(
                    node,
                    *features,
                    alias,
                    *timestamp,
                    addresses
                        .iter()
                        .map(|a| address::KnownAddress::new(a.clone(), address::Source::Peer)),
                ) {
                    Ok(updated) => {
                        // Only relay if we received new information.
                        if updated {
                            debug!("Address store entry for node {node} updated at {timestamp}");
                            return Ok(relay);
                        }
                    }
                    Err(err) => {
                        // An error here is due to a fault in our address store.
                        error!("Error processing node announcement from {node}: {err}");
                    }
                }
            }
        }
        Ok(false)
    }

    pub fn handle_message(
        &mut self,
        remote: &net::SocketAddr,
        envelope: Envelope,
    ) -> Result<Option<Announcement>, peer::SessionError> {
        let peer_ip = remote.ip();
        let peer = if let Some(peer) = self.sessions.get_mut(&peer_ip) {
            peer
        } else {
            return Err(SessionError::NotFound(remote.ip()));
        };

        if envelope.magic != self.config.network.magic() {
            return Err(SessionError::WrongMagic(envelope.magic));
        }
        debug!("Received {:?} from {}", &envelope.msg, peer.ip());

        match (&peer.state, envelope.msg) {
            (
                SessionState::Initial,
                Message::Initialize {
                    id,
                    version,
                    addrs,
                    git,
                },
            ) => {
                if version != PROTOCOL_VERSION {
                    return Err(SessionError::WrongVersion(version));
                }
                // Nb. This is a very primitive handshake. Eventually we should have anyhow
                // extra "acknowledgment" message sent when the `Initialize` is well received.
                if peer.link.is_inbound() {
                    self.reactor.write_all(
                        peer.addr,
                        gossip::handshake(
                            self.clock.timestamp(),
                            &self.storage,
                            &self.signer,
                            &self.config,
                        ),
                    );
                }
                // Nb. we don't set the peer timestamp here, since it is going to be
                // set after the first message is received only. Setting it here would
                // mean that messages received right after the handshake could be ignored.
                peer.state = SessionState::Negotiated {
                    id,
                    since: self.clock.local_time(),
                    addrs,
                    git,
                };
            }
            (SessionState::Initial, _) => {
                debug!(
                    "Disconnecting peer {} for sending us a message before handshake",
                    peer.ip()
                );
                return Err(SessionError::Misbehavior);
            }
            // Process a peer announcement.
            (SessionState::Negotiated { id, git, .. }, Message::Announcement(ann)) => {
                let git = git.clone();
                let id = *id;

                // Returning true here means that the message should be relayed.
                if self.handle_announcement(&id, &git, &ann)? {
                    self.gossip.received(ann.clone(), ann.message.timestamp());
                    return Ok(Some(ann));
                }
            }
            (SessionState::Negotiated { .. }, Message::Subscribe(subscribe)) => {
                for msg in self
                    .gossip
                    .filtered(&subscribe.filter, subscribe.since, subscribe.until)
                {
                    self.reactor.write(peer.addr, msg);
                }
                peer.subscribe = Some(subscribe);
            }
            (SessionState::Negotiated { .. }, Message::Initialize { .. }) => {
                debug!(
                    "Disconnecting peer {} for sending us a redundant handshake message",
                    peer.ip()
                );
                return Err(SessionError::Misbehavior);
            }
            (SessionState::Disconnected { .. }, msg) => {
                debug!("Ignoring {:?} from disconnected peer {}", msg, peer.ip());
            }
        }
        Ok(None)
    }

    /// Process a peer inventory announcement by updating our routing table.
    fn process_inventory(
        &mut self,
        inventory: &Inventory,
        from: NodeId,
        remote: &Url,
    ) -> Result<(), Error> {
        for proj_id in inventory {
            // TODO: Fire an event on routing update.
            // FIXME: The timestamp we insert should be the announcement timestamp.
            if self
                .routing
                .insert(*proj_id, from, self.clock.timestamp())?
                && self.config.is_tracking(proj_id)
            {
                self.storage.fetch(*proj_id, remote)?;
            }
        }
        Ok(())
    }

    /// Announce local refs for given id.
    fn announce_refs(&mut self, id: Id) -> Result<(), storage::Error> {
        let node = self.node_id();
        let repo = self.storage.repository(id)?;
        let remote = repo.remote(&node)?;
        let peers = self.sessions.negotiated().map(|(_, p)| p);
        let refs = remote.refs.into();
        let timestamp = self.clock.timestamp();
        let msg = AnnouncementMessage::from(RefsAnnouncement {
            id,
            refs,
            timestamp,
        });
        let ann = msg.signed(&self.signer);

        self.reactor.broadcast(ann, peers);

        Ok(())
    }

    ////////////////////////////////////////////////////////////////////////////
    // Periodic tasks
    ////////////////////////////////////////////////////////////////////////////

    /// Announce our inventory to all connected peers.
    fn announce_inventory(&mut self) -> Result<(), storage::Error> {
        let inventory = self.storage().inventory()?;
        let inv = Message::inventory(
            gossip::inventory(self.clock.timestamp(), inventory),
            &self.signer,
        );

        for addr in self.sessions.negotiated().map(|(_, p)| p.addr) {
            self.reactor.write(addr, inv.clone());
        }
        Ok(())
    }

    fn prune_routing_entries(&mut self) -> Result<(), storage::Error> {
        // TODO
        Ok(())
    }

    fn maintain_connections(&mut self) {
        // TODO: Connect to all potential seeds.
        if self.sessions.len() < TARGET_OUTBOUND_PEERS {
            let delta = TARGET_OUTBOUND_PEERS - self.sessions.len();

            for _ in 0..delta {
                // TODO: Connect to random peer.
            }
        }
    }
}

/// Gives read access to the service state.
pub trait ServiceState {
    /// Get the connected peers.
    fn sessions(&self) -> &Sessions;
    /// Get the current inventory.
    fn inventory(&self) -> Result<Inventory, storage::Error>;
    /// Get a project from storage, using the local node's key.
    fn get(&self, proj: Id) -> Result<Option<Doc<Verified>>, storage::ProjectError>;
    /// Get the clock.
    fn clock(&self) -> &RefClock;
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

    fn get(&self, proj: Id) -> Result<Option<Doc<Verified>>, storage::ProjectError> {
        self.storage.get(&self.node_id(), proj)
    }

    fn clock(&self) -> &RefClock {
        &self.clock
    }

    fn config(&self) -> &Config {
        &self.config
    }

    fn routing(&self) -> &dyn routing::Store {
        &self.routing
    }
}

#[derive(Debug)]
pub enum DisconnectReason {
    User,
    Error(SessionError),
}

impl DisconnectReason {
    fn is_transient(&self) -> bool {
        match self {
            Self::User => false,
            Self::Error(..) => false,
        }
    }
}

impl From<DisconnectReason> for nakamoto_net::DisconnectReason<DisconnectReason> {
    fn from(reason: DisconnectReason) -> Self {
        nakamoto_net::DisconnectReason::Protocol(reason)
    }
}

impl fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Error(err) => write!(f, "error: {}", err),
        }
    }
}

impl<R, A, S, G> Iterator for Service<R, A, S, G> {
    type Item = reactor::Io;

    fn next(&mut self) -> Option<Self::Item> {
        self.reactor.next()
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
    Project(#[from] storage::ProjectError),
}

/// Information on a peer, that we may or may not be connected to.
#[derive(Default, Debug)]
pub struct Peer {
    /// Timestamp of the last message received from peer.
    pub last_message: Timestamp,
}

#[derive(Debug)]
/// Holds currently (or recently) connected peers.
pub struct Sessions(AddressBook<IpAddr, Session>);

impl Sessions {
    pub fn new(rng: Rng) -> Self {
        Self(AddressBook::new(rng))
    }

    pub fn by_id(&self, id: &NodeId) -> Option<&Session> {
        self.0.values().find(|p| {
            if let SessionState::Negotiated { id: _id, .. } = &p.state {
                _id == id
            } else {
                false
            }
        })
    }

    /// Iterator over fully negotiated peers.
    pub fn negotiated(&self) -> impl Iterator<Item = (&IpAddr, &Session)> + Clone {
        self.0.iter().filter(move |(_, p)| p.is_negotiated())
    }
}

impl Deref for Sessions {
    type Target = AddressBook<IpAddr, Session>;

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
        ) -> impl Iterator<Item = Message> + '_ {
            self.received
                .iter()
                .filter(move |(t, _)| *t >= start && *t < end)
                .filter(move |(_, a)| a.matches(filter))
                .cloned()
                .map(|(_, a)| a.into())
        }
    }

    pub fn handshake<G: Signer, S: ReadStorage>(
        timestamp: Timestamp,
        storage: &S,
        signer: &G,
        config: &Config,
    ) -> [Message; 4] {
        let git = config.git_url.clone();
        let inventory = match storage.inventory() {
            Ok(i) => i,
            Err(e) => {
                error!("Error getting local inventory for handshake: {}", e);
                // Other than crashing the node completely, there's nothing we can do
                // here besides returning an empty inventory and logging an error.
                vec![]
            }
        };

        [
            Message::init(*signer.public_key(), config.listen.clone(), git),
            Message::node(gossip::node(timestamp, config), signer),
            Message::inventory(gossip::inventory(timestamp, inventory), signer),
            Message::subscribe(config.filter(), timestamp, Timestamp::MAX),
        ]
    }

    pub fn node(timestamp: Timestamp, config: &Config) -> NodeAnnouncement {
        let features = node::Features::SEED;
        let alias = config.alias();
        let addresses = vec![]; // TODO

        NodeAnnouncement {
            features,
            timestamp,
            alias,
            addresses,
        }
    }

    pub fn inventory(timestamp: Timestamp, inventory: Vec<Id>) -> InventoryAnnouncement {
        InventoryAnnouncement {
            inventory,
            timestamp,
        }
    }
}
