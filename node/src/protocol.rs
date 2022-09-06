#![allow(dead_code)]
pub mod config;
pub mod message;
pub mod peer;

use std::ops::{Deref, DerefMut};
use std::{collections::VecDeque, fmt, io, net, net::IpAddr};

use crossbeam_channel as chan;
use fastrand::Rng;
use git_url::Url;
use log::*;
use nakamoto::{LocalDuration, LocalTime};
use nakamoto_net as nakamoto;
use nakamoto_net::{Io, Link};
use nonempty::NonEmpty;

use crate::address_book;
use crate::address_book::AddressBook;
use crate::address_manager::AddressManager;
use crate::clock::RefClock;
use crate::collections::{HashMap, HashSet};
use crate::crypto;
use crate::identity::{ProjId, Project, UserId};
use crate::protocol::config::ProjectTracking;
use crate::protocol::message::Message;
use crate::protocol::peer::{Peer, PeerError, PeerState};
use crate::storage::{self, ReadRepository, WriteRepository};
use crate::storage::{Inventory, WriteStorage};

pub use crate::protocol::config::{Config, Network};

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
/// Network routing table. Keeps track of where projects are hosted.
pub type Routing = HashMap<ProjId, HashSet<NodeId>>;
/// Seconds since epoch.
pub type Timestamp = u64;

/// A protocol event.
#[derive(Debug, Clone)]
pub enum Event {}

/// Error returned by [`Command::Fetch`].
#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    Storage(#[from] storage::Error),
}

/// Result of looking up providers in our routing table.
#[derive(Debug)]
pub enum FetchLookup {
    Found {
        providers: NonEmpty<net::SocketAddr>,
        results: chan::Receiver<FetchResult>,
    },
    NotFound,
    Error(FetchError),
}

/// Result of a fetch request from a specific provider.
#[derive(Debug)]
pub enum FetchResult {
    /// Successful fetch from a provider.
    Fetched { from: net::SocketAddr },
    /// Error fetching the resource from a provider.
    Error {
        from: net::SocketAddr,
        error: FetchError,
    },
}

/// Commands sent to the protocol by the operator.
#[derive(Debug)]
pub enum Command {
    AnnounceRefsUpdate(ProjId),
    Connect(net::SocketAddr),
    Fetch(ProjId, chan::Sender<FetchLookup>),
    Track(ProjId, chan::Sender<bool>),
    Untrack(ProjId, chan::Sender<bool>),
}

/// Command-related errors.
#[derive(thiserror::Error, Debug)]
pub enum CommandError {}

#[derive(Debug)]
pub struct Protocol<S, T, G> {
    /// Peers currently or recently connected.
    peers: Peers,
    /// Protocol state that isn't peer-specific.
    context: Context<S, T, G>,
    /// Whether our local inventory no long represents what we have announced to the network.
    out_of_sync: bool,
    /// Last time the protocol was idle.
    last_idle: LocalTime,
    /// Last time the protocol synced.
    last_sync: LocalTime,
    /// Last time the protocol routing table was pruned.
    last_prune: LocalTime,
    /// Last time the protocol announced its inventory.
    last_announce: LocalTime,
    /// Time when the protocol was initialized.
    start_time: LocalTime,
}

impl<'r, T: WriteStorage<'r>, S: address_book::Store, G: crypto::Signer> Protocol<S, T, G> {
    pub fn new(
        config: Config,
        clock: RefClock,
        storage: T,
        addresses: S,
        signer: G,
        rng: Rng,
    ) -> Self {
        let addrmgr = AddressManager::new(addresses);

        Self {
            context: Context::new(config, clock, storage, addrmgr, signer, rng.clone()),
            peers: Peers::new(rng),
            out_of_sync: false,
            last_idle: LocalTime::default(),
            last_sync: LocalTime::default(),
            last_prune: LocalTime::default(),
            last_announce: LocalTime::default(),
            start_time: LocalTime::default(),
        }
    }

    pub fn disconnect(&mut self, remote: &IpAddr, reason: DisconnectReason) {
        if let Some(addr) = self.peers.get(remote).map(|p| p.addr) {
            self.context.disconnect(addr, reason);
        }
    }

    pub fn providers(&self, proj: &ProjId) -> Box<dyn Iterator<Item = (&NodeId, &Peer)> + '_> {
        if let Some(peers) = self.routing.get(proj) {
            Box::new(
                peers
                    .iter()
                    .filter_map(|id| self.peers.by_id(id).map(|p| (id, p))),
            )
        } else {
            Box::new(std::iter::empty())
        }
    }

    pub fn tracked(&self) -> Result<Vec<ProjId>, storage::Error> {
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
    pub fn track(&mut self, proj: ProjId) -> bool {
        self.out_of_sync = self.config.track(proj);
        self.out_of_sync
    }

    /// Untrack a project.
    /// Returns whether or not the tracking policy was updated.
    /// Note that when untracking, we don't announce anything to the network. This is because by
    /// simply not announcing it anymore, it will eventually be pruned by nodes.
    pub fn untrack(&mut self, proj: ProjId) -> bool {
        self.config.untrack(proj)
    }

    /// Find the closest `n` peers by proximity in tracking graphs.
    /// Returns a sorted list from the closest peer to the furthest.
    /// Peers with more trackings in common score score higher.
    #[allow(unused)]
    pub fn closest_peers(&self, n: usize) -> Vec<NodeId> {
        todo!()
    }

    /// Get the connected peers.
    pub fn peers(&self) -> &Peers {
        &self.peers
    }

    /// Get the current inventory.
    pub fn inventory(&self) -> Result<Inventory, storage::Error> {
        self.context.storage.inventory()
    }

    /// Get the storage instance.
    pub fn storage(&self) -> &T {
        &self.context.storage
    }

    /// Get the local protocol time.
    pub fn local_time(&self) -> LocalTime {
        self.context.clock.local_time()
    }

    /// Get protocol configuration.
    pub fn config(&self) -> &Config {
        &self.context.config
    }

    /// Get reference to routing table.
    pub fn routing(&self) -> &Routing {
        &self.context.routing
    }

    /// Get I/O outbox.
    pub fn outbox(&mut self) -> &mut VecDeque<Io<Event, DisconnectReason>> {
        &mut self.context.io
    }

    pub fn lookup(&self, proj: &ProjId) -> Lookup {
        Lookup {
            local: self.context.storage.get(proj).unwrap(),
            remote: self
                .context
                .routing
                .get(proj)
                .map_or(vec![], |r| r.iter().cloned().collect()),
        }
    }

    ////////////////////////////////////////////////////////////////////////////
    // Periodic tasks
    ////////////////////////////////////////////////////////////////////////////

    /// Announce our inventory to all connected peers.
    fn announce_inventory(&mut self) -> Result<(), storage::Error> {
        let inv = Message::inventory(&self.context)?;

        for addr in self.peers.negotiated().map(|(_, p)| p.addr) {
            self.context.write(addr, inv.clone());
        }
        Ok(())
    }

    fn get_inventories(&mut self) -> Result<(), storage::Error> {
        let mut msgs = Vec::new();
        for proj in self.tracked()? {
            for (_, peer) in self.providers(&proj) {
                if peer.is_negotiated() {
                    msgs.push((
                        peer.addr,
                        Message::GetInventory {
                            ids: vec![proj.clone()],
                        },
                    ));
                }
            }
        }
        for (remote, msg) in msgs {
            self.write(remote, msg);
        }

        Ok(())
    }

    fn prune_routing_entries(&mut self) {
        // TODO
    }

    fn maintain_connections(&mut self) {
        // TODO: Connect to all potential providers.
        if self.peers.len() < TARGET_OUTBOUND_PEERS {
            let delta = TARGET_OUTBOUND_PEERS - self.peers.len();

            for _ in 0..delta {
                // TODO: Connect to random peer.
            }
        }
    }
}

impl<'r, S, T, G> nakamoto::Protocol for Protocol<S, T, G>
where
    T: WriteStorage<'r> + 'static,
    S: address_book::Store,
    G: crypto::Signer,
{
    type Event = Event;
    type Command = Command;
    type DisconnectReason = DisconnectReason;

    fn initialize(&mut self, time: LocalTime) {
        trace!("Init {}", time.as_secs());

        self.start_time = time;

        // Connect to configured peers.
        let addrs = self.context.config.connect.clone();
        for addr in addrs {
            self.context.connect(addr);
        }
    }

    fn tick(&mut self, now: nakamoto::LocalTime) {
        trace!("Tick +{}", now - self.start_time);

        self.context.clock.set(now);
    }

    fn wake(&mut self) {
        let now = self.context.clock.local_time();

        trace!("Wake +{}", now - self.start_time);

        if now - self.last_idle >= IDLE_INTERVAL {
            debug!("Running 'idle' task...");

            self.maintain_connections();
            self.context.io.push_back(Io::Wakeup(IDLE_INTERVAL));
            self.last_idle = now;
        }
        if now - self.last_sync >= SYNC_INTERVAL {
            debug!("Running 'sync' task...");

            self.get_inventories().unwrap();
            self.context.io.push_back(Io::Wakeup(SYNC_INTERVAL));
            self.last_sync = now;
        }
        if now - self.last_announce >= ANNOUNCE_INTERVAL {
            if self.out_of_sync {
                self.announce_inventory().unwrap();
            }
            self.context.io.push_back(Io::Wakeup(ANNOUNCE_INTERVAL));
            self.last_announce = now;
        }
        if now - self.last_prune >= PRUNE_INTERVAL {
            debug!("Running 'prune' task...");

            self.prune_routing_entries();
            self.context.io.push_back(Io::Wakeup(PRUNE_INTERVAL));
            self.last_prune = now;
        }
    }

    fn command(&mut self, cmd: Self::Command) {
        debug!("Command {:?}", cmd);

        match cmd {
            Command::Connect(addr) => self.context.connect(addr),
            Command::Fetch(proj, resp) => {
                let providers = self.providers(&proj).collect::<Vec<_>>();
                let providers = if let Some(providers) = NonEmpty::from_vec(providers) {
                    providers
                } else {
                    log::error!("No providers found for {}", proj);
                    resp.send(FetchLookup::NotFound).ok();

                    return;
                };
                log::debug!("Found {} providers for {}", providers.len(), proj);

                let mut repo = match self.storage.repository(&proj) {
                    Ok(repo) => repo,
                    Err(err) => {
                        log::error!("Error opening repo for {}: {}", proj, err);
                        resp.send(FetchLookup::Error(err.into())).ok();

                        return;
                    }
                };

                let (results_, results) = chan::bounded(providers.len());
                resp.send(FetchLookup::Found {
                    providers: providers.clone().map(|(_, peer)| peer.addr),
                    results,
                })
                .ok();

                // TODO: Limit the number of providers we fetch from? Randomize?
                for (_, peer) in providers {
                    match repo.fetch(&Url {
                        scheme: git_url::Scheme::Git,
                        host: Some(peer.addr.ip().to_string()),
                        port: Some(peer.addr.port()),
                        // TODO: Fix upstream crate so that it adds a `/` when needed.
                        path: format!("/{}", proj).into(),
                        ..Url::default()
                    }) {
                        Ok(()) => {
                            results_.send(FetchResult::Fetched { from: peer.addr }).ok();
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
            Command::Track(proj, resp) => {
                resp.send(self.track(proj)).ok();
            }
            Command::Untrack(proj, resp) => {
                resp.send(self.untrack(proj)).ok();
            }
            Command::AnnounceRefsUpdate(proj) => {
                let user = *self.storage.user_id();
                let repo = self.storage.repository(&proj).unwrap();
                let remote = repo.remote(&user).unwrap();
                let peers = self.peers.negotiated().map(|(_, p)| p.addr);
                let refs = remote.refs.unverified();

                self.context
                    .broadcast(Message::RefsUpdate { proj, user, refs }, peers);
            }
        }
    }

    fn attempted(&mut self, addr: &std::net::SocketAddr) {
        let ip = addr.ip();
        let persistent = self.context.config.is_persistent(addr);
        let peer = self
            .peers
            .entry(ip)
            .or_insert_with(|| Peer::new(*addr, Link::Outbound, persistent));

        peer.attempted();
    }

    fn connected(
        &mut self,
        addr: std::net::SocketAddr,
        _local_addr: &std::net::SocketAddr,
        link: Link,
    ) {
        let ip = addr.ip();

        debug!("Connected to {} ({:?})", ip, link);

        // For outbound connections, we are the first to say "Hello".
        // For inbound connections, we wait for the remote to say "Hello" first.
        // TODO: How should we deal with multiple peers connecting from the same IP address?
        if link.is_outbound() {
            let git = self.config.git_url.clone();

            if let Some(peer) = self.peers.get_mut(&ip) {
                self.context.write_all(
                    peer.addr,
                    [
                        Message::hello(
                            self.context.id(),
                            self.context.timestamp(),
                            self.context.config.listen.clone(),
                            git,
                        ),
                        Message::get_inventory([]),
                    ],
                );
                peer.connected();
            }
        } else {
            self.peers.insert(
                ip,
                Peer::new(
                    addr,
                    Link::Inbound,
                    self.context.config.is_persistent(&addr),
                ),
            );
        }
    }

    fn disconnected(
        &mut self,
        addr: &std::net::SocketAddr,
        reason: nakamoto::DisconnectReason<Self::DisconnectReason>,
    ) {
        let since = self.local_time();
        let ip = addr.ip();

        debug!("Disconnected from {} ({})", ip, reason);

        if let Some(peer) = self.peers.get_mut(&ip) {
            peer.state = PeerState::Disconnected { since };

            // Attempt to re-connect to persistent peers.
            if self.context.config.is_persistent(addr) && peer.attempts() < MAX_CONNECTION_ATTEMPTS
            {
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

                self.context.connect(*addr);
            } else {
                // TODO: Non-persistent peers should be removed from the
                // map here or at some later point.
            }
        }
    }

    fn received_bytes(&mut self, addr: &std::net::SocketAddr, bytes: &[u8]) {
        let peer = addr.ip();
        let negotiated = self
            .peers
            .negotiated()
            .map(|(id, p)| (*id, p.addr))
            .collect::<Vec<_>>();

        let (peer, msgs) = if let Some(peer) = self.peers.get_mut(&peer) {
            let decoder = &mut peer.inbox();
            decoder.input(bytes);

            let mut msgs = Vec::with_capacity(1);
            loop {
                match decoder.decode_next() {
                    Ok(Some(msg)) => msgs.push(msg),
                    Ok(None) => break,

                    Err(_err) => {
                        // TODO: Disconnect peer.
                        return;
                    }
                }
            }
            (peer, msgs)
        } else {
            return;
        };

        for msg in msgs {
            match peer.received(msg, &mut self.context) {
                Ok(None) => {}
                Ok(Some(msg)) => {
                    let peers = negotiated
                        .iter()
                        .filter(|(ip, _)| *ip != peer.ip())
                        .map(|(_, addr)| *addr);

                    self.context.broadcast(msg, peers);
                }
                Err(err) => {
                    self.context
                        .disconnect(peer.addr, DisconnectReason::Error(err));
                }
            }
        }
    }
}

impl<S, T, G> Deref for Protocol<S, T, G> {
    type Target = Context<S, T, G>;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl<S, T, G> DerefMut for Protocol<S, T, G> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.context
    }
}

#[derive(Debug, Clone)]
pub enum DisconnectReason {
    User,
    Error(PeerError),
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

impl<S, T, G> Iterator for Protocol<S, T, G> {
    type Item = Io<Event, DisconnectReason>;

    fn next(&mut self) -> Option<Self::Item> {
        self.context.io.pop_front()
    }
}

/// Result of a project lookup.
#[derive(Debug)]
pub struct Lookup {
    /// Whether the project was found locally or not.
    pub local: Option<Project>,
    /// A list of remote peers on which the project is known to exist.
    pub remote: Vec<NodeId>,
}

/// Global protocol state used across peers.
#[derive(Debug)]
pub struct Context<S, T, G> {
    /// Protocol configuration.
    config: Config,
    /// Our cryptographic signer and key.
    signer: G,
    /// Tracks the location of projects.
    routing: Routing,
    /// Outgoing I/O queue.
    io: VecDeque<Io<Event, DisconnectReason>>,
    /// Clock. Tells the time.
    clock: RefClock,
    /// Project storage.
    storage: T,
    /// Peer address manager.
    addrmgr: AddressManager<S>,
    /// Source of entropy.
    rng: Rng,
}

impl<'r, S, T, G> Context<S, T, G>
where
    T: storage::WriteStorage<'r>,
    G: crypto::Signer,
{
    pub(crate) fn new(
        config: Config,
        clock: RefClock,
        storage: T,
        addrmgr: AddressManager<S>,
        signer: G,
        rng: Rng,
    ) -> Self {
        Self {
            config,
            signer,
            clock,
            routing: HashMap::with_hasher(rng.clone().into()),
            io: VecDeque::new(),
            storage,
            addrmgr,
            rng,
        }
    }

    pub(crate) fn id(&self) -> NodeId {
        *self.signer.public_key()
    }

    /// Process a peer inventory announcement by updating our routing table.
    fn process_inventory(&mut self, inventory: &Inventory, from: NodeId, remote: &Url) {
        for proj_id in inventory {
            let inventory = self
                .routing
                .entry(proj_id.clone())
                .or_insert_with(|| HashSet::with_hasher(self.rng.clone().into()));

            // TODO: Fire an event on routing update.
            if inventory.insert(from) && self.config.is_tracking(proj_id) {
                self.fetch(proj_id, remote);
            }
        }
    }

    /// Process a peer inventory update announcement by (maybe) fetching.
    fn process_refs_update(&mut self, proj: &ProjId, _user: &UserId, remote: &Url) -> bool {
        // TODO: Check that we're tracking this user as well.
        if self.config.is_tracking(proj) {
            self.fetch(proj, remote);
        }
        // TODO: If refs were updated, return `true`.
        false
    }

    fn fetch(&mut self, proj_id: &ProjId, remote: &Url) {
        // TODO: Verify refs before adding them to storage.
        let mut repo = self.storage.repository(proj_id).unwrap();
        repo.fetch(&Url {
            path: format!("/{}", proj_id).into(),
            ..remote.clone()
        })
        .unwrap();
    }

    /// Disconnect a peer.
    fn disconnect(&mut self, addr: net::SocketAddr, reason: DisconnectReason) {
        self.io.push_back(Io::Disconnect(addr, reason));
    }
}

impl<S, T, G> Context<S, T, G> {
    /// Get current local timestamp.
    pub(crate) fn timestamp(&self) -> Timestamp {
        self.clock.local_time().as_secs()
    }

    /// Connect to a peer.
    fn connect(&mut self, addr: net::SocketAddr) {
        // TODO: Make sure we don't try to connect more than once to the same address.
        self.io.push_back(Io::Connect(addr));
    }

    fn write_all(&mut self, remote: net::SocketAddr, msgs: impl IntoIterator<Item = Message>) {
        let mut buf = io::Cursor::new(Vec::new());

        for msg in msgs {
            debug!("Write {:?} to {}", &msg, remote.ip());

            let envelope = self.config.network.envelope(msg);
            serde_json::to_writer(&mut buf, &envelope).unwrap();
        }
        self.io.push_back(Io::Write(remote, buf.into_inner()));
    }

    fn write(&mut self, remote: net::SocketAddr, msg: Message) {
        debug!("Write {:?} to {}", &msg, remote.ip());

        let envelope = self.config.network.envelope(msg);
        let bytes = serde_json::to_vec(&envelope).unwrap();

        self.io.push_back(Io::Write(remote, bytes));
    }

    /// Broadcast a message to a list of peers.
    fn broadcast(&mut self, msg: Message, peers: impl IntoIterator<Item = net::SocketAddr>) {
        for peer in peers {
            self.write(peer, msg.clone());
        }
    }
}

#[derive(Debug)]
/// Holds currently (or recently) connected peers.
pub struct Peers(AddressBook<IpAddr, Peer>);

impl Peers {
    pub fn new(rng: Rng) -> Self {
        Self(AddressBook::new(rng))
    }

    pub fn by_id(&self, id: &NodeId) -> Option<&Peer> {
        self.0.values().find(|p| {
            if let PeerState::Negotiated { id: _id, .. } = &p.state {
                _id == id
            } else {
                false
            }
        })
    }

    /// Iterator over fully negotiated peers.
    pub fn negotiated(&self) -> impl Iterator<Item = (&IpAddr, &Peer)> + Clone {
        self.0.iter().filter(move |(_, p)| p.is_negotiated())
    }
}

impl Deref for Peers {
    type Target = AddressBook<IpAddr, Peer>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Peers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
