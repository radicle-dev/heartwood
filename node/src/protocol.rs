#![allow(dead_code)]
use std::ops::{Deref, DerefMut};
use std::{collections::VecDeque, fmt, io, net, net::IpAddr};

use fastrand::Rng;
use git_url::Url;
use log::*;
use nakamoto::{LocalDuration, LocalTime};
use nakamoto_net as nakamoto;
use nakamoto_net::{Io, Link};
use serde::{Deserialize, Serialize};

use crate::address_book;
use crate::address_book::AddressBook;
use crate::address_manager::AddressManager;
use crate::clock::RefClock;
use crate::collections::{HashMap, HashSet};
use crate::decoder::Decoder;
use crate::git;
use crate::identity::{ProjId, UserId};
use crate::storage;
use crate::storage::{Inventory, ReadStorage, Remotes, Unverified, WriteStorage};

/// Network peer identifier.
pub type PeerId = IpAddr;
/// Network routing table. Keeps track of where projects are hosted.
pub type Routing = HashMap<ProjId, HashSet<PeerId>>;
/// Seconds since epoch.
pub type Timestamp = u64;

pub const NETWORK_MAGIC: u32 = 0x819b43d9;
pub const DEFAULT_PORT: u16 = 8776;
pub const PROTOCOL_VERSION: u32 = 1;
pub const TARGET_OUTBOUND_PEERS: usize = 8;
pub const IDLE_INTERVAL: LocalDuration = LocalDuration::from_secs(30);
pub const ANNOUNCE_INTERVAL: LocalDuration = LocalDuration::from_secs(30);
pub const SYNC_INTERVAL: LocalDuration = LocalDuration::from_secs(60);
pub const PRUNE_INTERVAL: LocalDuration = LocalDuration::from_mins(30);
pub const MAX_CONNECTION_ATTEMPTS: usize = 3;
pub const MAX_TIME_DELTA: LocalDuration = LocalDuration::from_mins(60);

/// Commands sent to the protocol by the operator.
#[derive(Debug)]
pub enum Command {
    Connect(net::SocketAddr),
    Fetch(ProjId, net::SocketAddr),
}

/// Message envelope. All messages sent over the network are wrapped in this type.
#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope {
    /// Network magic constant. Used to differentiate networks.
    pub magic: u32,
    /// The message payload.
    pub msg: Message,
}

/// Message payload.
/// These are the messages peers send to each other.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Message {
    /// Say hello to a peer. This is the first message sent to a peer after connection.
    Hello { version: u32, git: Url },
    /// Get node addresses from a peer.
    GetAddrs,
    /// Send node addresses to a peer. Sent in response to [`Message::GetAddrs`].
    Addrs { addrs: Vec<net::SocketAddr> },
    /// Get a peer's inventory.
    GetInventory { ids: Vec<ProjId> },
    /// Send our inventory to a peer. Sent in response to [`Message::GetInventory`].
    /// Nb. This should be the whole inventory, not a partial update.
    Inventory {
        inv: Inventory,
        timestamp: Timestamp,
        /// Original peer this inventory came from. We don't set this when we
        /// are the originator, only when relaying.
        origin: Option<PeerId>,
    },
}

impl From<Message> for Envelope {
    fn from(msg: Message) -> Self {
        Self {
            magic: NETWORK_MAGIC,
            msg,
        }
    }
}

impl Message {
    pub fn hello(git: Url) -> Self {
        Self::Hello {
            version: PROTOCOL_VERSION,
            git,
        }
    }

    pub fn inventory<S, T>(ctx: &mut Context<S, T>) -> Result<Self, storage::Error>
    where
        T: storage::ReadStorage,
    {
        let timestamp = ctx.timestamp();
        let inv = ctx.storage.inventory()?;

        Ok(Self::Inventory {
            timestamp,
            inv,
            origin: None,
        })
    }

    pub fn get_inventory(ids: impl Into<Vec<ProjId>>) -> Self {
        Self::GetInventory { ids: ids.into() }
    }
}

/// Project tracking policy.
#[derive(Debug)]
pub enum ProjectTracking {
    /// Track all projects we come across.
    All { blocked: HashSet<ProjId> },
    /// Track a static list of projects.
    Allowed(HashSet<ProjId>),
}

impl Default for ProjectTracking {
    fn default() -> Self {
        Self::All {
            blocked: HashSet::default(),
        }
    }
}

/// Project remote tracking policy.
#[derive(Debug, Default)]
pub enum RemoteTracking {
    /// Only track remotes of project delegates.
    #[default]
    DelegatesOnly,
    /// Track all remotes.
    All { blocked: HashSet<UserId> },
    /// Track a specific list of users as well as the project delegates.
    Allowed(HashSet<UserId>),
}

/// Protocol configuration.
#[derive(Debug)]
pub struct Config {
    /// Peers to connect to on startup.
    /// Connections to these peers will be maintained.
    pub connect: Vec<net::SocketAddr>,
    /// Project tracking policy.
    pub project_tracking: ProjectTracking,
    /// Project remote tracking policy.
    pub remote_tracking: RemoteTracking,
    /// Whether or not our node should relay inventories.
    pub relay: bool,
    /// Our Git URL for fetching projects.
    pub git_url: Url,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connect: Vec::default(),
            project_tracking: ProjectTracking::default(),
            remote_tracking: RemoteTracking::default(),
            relay: true,
            git_url: Url::default(),
        }
    }
}

impl Config {
    pub fn is_persistent(&self, addr: &net::SocketAddr) -> bool {
        self.connect.contains(addr)
    }

    /// Track a project. Returns whether the policy was updated.
    pub fn track(&mut self, proj: ProjId) -> bool {
        match &mut self.project_tracking {
            ProjectTracking::All { .. } => false,
            ProjectTracking::Allowed(projs) => projs.insert(proj),
        }
    }

    /// Untrack a project. Returns whether the policy was updated.
    pub fn untrack(&mut self, proj: ProjId) -> bool {
        match &mut self.project_tracking {
            ProjectTracking::All { blocked } => blocked.insert(proj),
            ProjectTracking::Allowed(projs) => projs.remove(&proj),
        }
    }
}

#[derive(Debug)]
pub struct Protocol<S, T> {
    /// Peers currently or recently connected.
    peers: Peers,
    /// Protocol state that isn't peer-specific.
    context: Context<S, T>,
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

impl<T: ReadStorage + WriteStorage, S: address_book::Store> Protocol<S, T> {
    pub fn new(config: Config, clock: RefClock, storage: T, addresses: S, rng: Rng) -> Self {
        let addrmgr = AddressManager::new(addresses);

        Self {
            context: Context::new(config, clock, storage, addrmgr, rng.clone()),
            peers: Peers::new(rng),
            out_of_sync: false,
            last_idle: LocalTime::default(),
            last_sync: LocalTime::default(),
            last_prune: LocalTime::default(),
            last_announce: LocalTime::default(),
            start_time: LocalTime::default(),
        }
    }

    pub fn disconnect(&mut self, peer: &PeerId, reason: DisconnectReason) {
        if let Some(addr) = self.peers.get(peer).map(|p| p.addr) {
            self.context.disconnect(addr, reason);
        }
    }

    pub fn providers(&self, proj: &ProjId) -> Box<dyn Iterator<Item = &Peer> + '_> {
        if let Some(peers) = self.routing.get(proj) {
            Box::new(peers.iter().filter_map(|id| self.peers.get(id)))
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
                .map(|(id, _)| id)
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
    pub fn closest_peers(&self, n: usize) -> Vec<PeerId> {
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
    pub fn outbox(&mut self) -> &mut VecDeque<Io<(), DisconnectReason>> {
        &mut self.context.io
    }

    pub fn lookup(&self, proj: &ProjId) -> Lookup {
        Lookup {
            local: self.context.storage.get(proj).unwrap(),
            remote: self
                .context
                .routing
                .get(proj)
                .map_or(vec![], |r| r.iter().copied().collect()),
        }
    }

    ////////////////////////////////////////////////////////////////////////////
    // Periodic tasks
    ////////////////////////////////////////////////////////////////////////////

    /// Announce our inventory to all connected peers.
    fn announce_inventory(&mut self) -> Result<(), storage::Error> {
        let inv = Message::inventory(&mut self.context)?;

        for addr in self.peers.negotiated().map(|(_, p)| p.addr) {
            self.context.write(addr, inv.clone());
        }
        Ok(())
    }

    fn get_inventories(&mut self) -> Result<(), storage::Error> {
        let mut msgs = Vec::new();
        for proj in self.tracked()? {
            for peer in self.providers(&proj) {
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

impl<S, T> nakamoto::Protocol for Protocol<S, T>
where
    T: ReadStorage + WriteStorage + 'static,
    S: address_book::Store,
{
    type Event = ();
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
            Command::Fetch(proj, remote) => {
                git::fetch(&proj, &format!("git://{}", remote), &mut self.storage).unwrap();
            }
        }
    }

    fn attempted(&mut self, addr: &std::net::SocketAddr) {
        let id = addr.ip();
        let persistent = self.context.config.is_persistent(addr);
        let mut peer = self
            .peers
            .entry(id)
            .or_insert_with(|| Peer::new(*addr, Link::Outbound, persistent));

        peer.attempts += 1;
    }

    fn connected(
        &mut self,
        addr: std::net::SocketAddr,
        _local_addr: &std::net::SocketAddr,
        link: Link,
    ) {
        let id = addr.ip();

        debug!("Connected to {} ({:?})", id, link);

        // For outbound connections, we are the first to say "Hello".
        // For inbound connections, we wait for the remote to say "Hello" first.
        // TODO: How should we deal with multiple peers connecting from the same IP address?
        if link.is_outbound() {
            let git = self.config.git_url.clone();

            if let Some(peer) = self.peers.get_mut(&id) {
                self.context
                    .write_all(peer.addr, [Message::hello(git), Message::get_inventory([])]);

                peer.attempts = 0;
            }
        } else {
            self.peers.insert(
                id,
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
        let id = addr.ip();

        debug!("Disconnected from {} ({})", id, reason);

        if let Some(peer) = self.peers.get_mut(&id) {
            peer.state = PeerState::Disconnected { since };

            // Attempt to re-connect to persistent peers.
            if self.context.config.is_persistent(addr) && peer.attempts < MAX_CONNECTION_ATTEMPTS {
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
                debug!("Reconnecting to {} (attempts={})...", id, peer.attempts);

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
            let decoder = &mut peer.inbox;
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
                        .filter(|(id, _)| *id != peer.id())
                        .map(|(_, addr)| *addr)
                        .collect::<Vec<_>>();

                    self.context.broadcast(msg, &peers);
                }
                Err(err) => {
                    self.context
                        .disconnect(peer.addr, DisconnectReason::Error(err));
                }
            }
        }
    }
}

impl<S, T> Deref for Protocol<S, T> {
    type Target = Context<S, T>;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl<S, T> DerefMut for Protocol<S, T> {
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

impl<S, T> Iterator for Protocol<S, T> {
    type Item = Io<(), DisconnectReason>;

    fn next(&mut self) -> Option<Self::Item> {
        self.context.io.pop_front()
    }
}

/// Result of a project lookup.
#[derive(Debug)]
pub struct Lookup {
    /// Whether the project was found locally or not.
    pub local: Option<Remotes<Unverified>>,
    /// A list of remote peers on which the project is known to exist.
    pub remote: Vec<PeerId>,
}

/// Global protocol state used across peers.
#[derive(Debug)]
pub struct Context<S, T> {
    /// Protocol configuration.
    config: Config,
    /// Tracks the location of projects.
    routing: Routing,
    /// Outgoing I/O queue.
    io: VecDeque<Io<(), DisconnectReason>>,
    /// Clock. Tells the time.
    clock: RefClock,
    /// Timestamps of known peers.
    timestamps: HashMap<PeerId, u64>,
    /// Project storage.
    storage: T,
    /// Peer address manager.
    addrmgr: AddressManager<S>,
    /// Source of entropy.
    rng: Rng,
}

impl<S, T> Context<S, T>
where
    T: storage::ReadStorage,
{
    fn new(
        config: Config,
        clock: RefClock,
        storage: T,
        addrmgr: AddressManager<S>,
        rng: Rng,
    ) -> Self {
        Self {
            config,
            clock,
            routing: HashMap::with_hasher(rng.clone().into()),
            timestamps: HashMap::with_hasher(rng.clone().into()),
            io: VecDeque::new(),
            storage,
            addrmgr,
            rng,
        }
    }

    /// Process a peer inventory announcement by updating our routing table.
    fn process_inventory(&mut self, inventory: &Inventory, from: PeerId) {
        for (proj_id, _refs) in inventory {
            let inventory = self
                .routing
                .entry(proj_id.clone())
                .or_insert_with(|| HashSet::with_hasher(self.rng.clone().into()));

            // TODO: If we're tracking this project, check the refs to see if we need to
            // fetch updates from this peer.

            inventory.insert(from);
        }
    }

    /// Get current local timestamp.
    fn timestamp(&self) -> Timestamp {
        self.clock.local_time().as_secs()
    }

    /// Disconnect a peer.
    fn disconnect(&mut self, addr: net::SocketAddr, reason: DisconnectReason) {
        self.io.push_back(Io::Disconnect(addr, reason));
    }
}

impl<S, T> Context<S, T> {
    /// Connect to a peer.
    fn connect(&mut self, addr: net::SocketAddr) {
        // TODO: Make sure we don't try to connect more than once to the same address.
        self.io.push_back(Io::Connect(addr));
    }

    fn write_all(&mut self, remote: net::SocketAddr, msgs: impl IntoIterator<Item = Message>) {
        let mut buf = io::Cursor::new(Vec::new());

        for msg in msgs {
            debug!("Write {:?} to {}", &msg, remote.ip());

            serde_json::to_writer(&mut buf, &Envelope::from(msg)).unwrap();
        }
        self.io.push_back(Io::Write(remote, buf.into_inner()));
    }

    fn write(&mut self, remote: net::SocketAddr, msg: Message) {
        debug!("Write {:?} to {}", &msg, remote.ip());

        let bytes = serde_json::to_vec(&Envelope::from(msg)).unwrap();

        self.io.push_back(Io::Write(remote, bytes));
    }

    /// Broadcast a message to a list of peers.
    fn broadcast(&mut self, msg: Message, peers: &[net::SocketAddr]) {
        for peer in peers {
            self.write(*peer, msg.clone());
        }
    }
}

#[derive(Debug)]
pub struct Peers(AddressBook<PeerId, Peer>);

impl Peers {
    pub fn new(rng: Rng) -> Self {
        Self(AddressBook::new(rng))
    }

    /// Iterator over fully negotiated peers.
    pub fn negotiated(&self) -> impl Iterator<Item = (&IpAddr, &Peer)> + Clone {
        self.0.iter().filter(move |(_, p)| p.is_negotiated())
    }
}

impl Deref for Peers {
    type Target = AddressBook<PeerId, Peer>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Peers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Default)]
enum PeerState {
    /// Initial peer state. For outgoing peers this
    /// means we've attempted a connection. For incoming
    /// peers, this means they've successfully connected
    /// to us.
    #[default]
    Initial,
    /// State after successful handshake.
    Negotiated { since: LocalTime, git: Url },
    /// When a peer is disconnected.
    Disconnected { since: LocalTime },
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum PeerError {
    #[error("wrong network constant in message: {0}")]
    WrongMagic(u32),
    #[error("wrong protocol version in message: {0}")]
    WrongVersion(u32),
    #[error("invalid inventory timestamp: {0}")]
    InvalidTimestamp(u64),
}

#[derive(Debug)]
pub struct Peer {
    /// Peer address.
    addr: net::SocketAddr,
    /// Inbox for incoming messages from peer.
    inbox: Decoder,
    /// Peer connection state.
    state: PeerState,
    /// Connection direction.
    link: Link,
    /// Whether we should attempt to re-connect
    /// to this peer upon disconnection.
    persistent: bool,
    /// Connection attempts. For persistent peers, Tracks
    /// how many times we've attempted to connect. We reset this to zero
    /// upon successful connection.
    attempts: usize,
}

impl Peer {
    fn new(addr: net::SocketAddr, link: Link, persistent: bool) -> Self {
        Self {
            addr,
            inbox: Decoder::new(256),
            state: PeerState::default(),
            link,
            persistent,
            attempts: 0,
        }
    }

    fn id(&self) -> PeerId {
        self.addr.ip()
    }

    fn is_negotiated(&self) -> bool {
        matches!(self.state, PeerState::Negotiated { .. })
    }

    fn received<S, T>(
        &mut self,
        envelope: Envelope,
        ctx: &mut Context<S, T>,
    ) -> Result<Option<Message>, PeerError>
    where
        T: storage::ReadStorage,
    {
        if envelope.magic != NETWORK_MAGIC {
            return Err(PeerError::WrongMagic(envelope.magic));
        }
        debug!("Received {:?} from {}", &envelope.msg, self.id());

        match envelope.msg {
            Message::Hello { version, git } => {
                if version != PROTOCOL_VERSION {
                    return Err(PeerError::WrongVersion(version));
                }
                if let PeerState::Initial = self.state {
                    // Nb. This is a very primitive handshake. Eventually we should have anyhow
                    // extra "acknowledgment" message sent when the `Hello` is well received.
                    if self.link.is_inbound() {
                        let git = ctx.config.git_url.clone();
                        ctx.write_all(self.addr, [Message::hello(git), Message::get_inventory([])]);
                    }
                    self.state = PeerState::Negotiated {
                        since: ctx.clock.local_time(),
                        git,
                    };
                } else {
                    // TODO: Handle misbehavior.
                }
            }
            Message::GetInventory { .. } => {
                // TODO: Handle partial inventory requests.
                let inventory = Message::inventory(ctx).unwrap();
                ctx.write(self.addr, inventory);
            }
            Message::Inventory {
                timestamp,
                inv,
                origin,
            } => {
                let now = ctx.clock.local_time();
                let last = ctx
                    .timestamps
                    .entry(self.id())
                    .or_insert_with(Timestamp::default);

                // Don't allow messages from too far in the past or future.
                if timestamp.abs_diff(now.as_secs()) > MAX_TIME_DELTA.as_secs() {
                    return Err(PeerError::InvalidTimestamp(timestamp));
                }
                // Discard inventory messages we've already seen, otherwise update
                // out last seen time.
                if timestamp > *last {
                    *last = timestamp;
                } else {
                    return Ok(None);
                }
                ctx.process_inventory(&inv, origin.unwrap_or_else(|| self.id()));

                if ctx.config.relay {
                    return Ok(Some(Message::Inventory {
                        timestamp,
                        inv,
                        origin: origin.or_else(|| Some(self.id())),
                    }));
                }
            }
            Message::GetAddrs => {
                // TODO: Send peer addresses.
                todo!();
            }
            Message::Addrs { .. } => {
                // TODO: Update address book.
                todo!();
            }
        }
        Ok(None)
    }
}
