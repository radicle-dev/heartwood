use std::collections::HashSet;
use std::iter;
use std::net;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use localtime::{LocalDuration, LocalTime};
use log::*;
use protocol::bounded::BoundedVec;
use protocol::service;
use protocol::service::io::Io;
use protocol::service::message::*;
use protocol::service::*;
use protocol::wire::MessageType;
use radicle::Storage;
use radicle::crypto::{Signer as _, SigningKey};
use radicle::git::Oid;
use radicle::identity::RepoId;
use radicle::identity::Visibility;
use radicle::node;
use radicle::node::Database;
use radicle::node::Link;
use radicle::node::PROTOCOL_VERSION;
use radicle::node::UserAgent;
use radicle::node::address::Store as _;
use radicle::node::events::Emitter;
use radicle::node::events::Events;
use radicle::node::policy::config as policy;
use radicle::node::policy::{Scope, SeedingPolicy};
use radicle::node::routing::Store as _;
use radicle::node::{Address, Event, NodeId, Timestamp};
use radicle::node::{Alias, ConnectOptions, address};
use radicle::rad;
use radicle::storage::WriteStorage;
use radicle::storage::refs;
use radicle::storage::refs::{RefsAt, SignedRefs};
use radicle::storage::{ReadRepository, RemoteRepository};
use radicle::test::storage::MockStorage;
use radicle::test::{arbitrary, fixtures};

/// Service instantiation used for testing.
pub type Service<S> = service::Service<Database, S>;

pub const AMY: u8 = 0x0A;
pub const BOB: u8 = 0x0B;
pub const CID: u8 = 0x0C;
pub const DAN: u8 = 0x0D;
pub const EVE: u8 = 0x0E;

#[derive(Debug)]
pub struct Peer<S> {
    name: &'static str,
    service: Service<S>,
    addr: net::SocketAddr,
    tempdir: tempfile::TempDir,
}

impl<S> Peer<S> {
    pub fn address(&self) -> Address {
        Address::from(self.addr)
    }
}

impl<S> Deref for Peer<S> {
    type Target = Service<S>;

    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

impl<S> DerefMut for Peer<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.service
    }
}

impl Peer<MockStorage> {
    pub fn amy() -> Self {
        Peer::new_empty_storage("amy", AMY)
    }

    pub fn amy_with(f: impl FnOnce(&mut Config)) -> Self {
        Peer::new_empty_with("amy", AMY, f)
    }

    pub fn bob() -> Self {
        Peer::new_empty_storage("bob", BOB)
    }

    pub fn bob_with(f: impl FnOnce(&mut Config)) -> Self {
        Peer::new_empty_with("bob", BOB, f)
    }

    pub fn cid() -> Self {
        Peer::new_empty_storage("cid", CID)
    }

    pub fn dan() -> Self {
        Peer::new_empty_storage("dan", DAN)
    }

    pub fn eve() -> Self {
        Peer::new_empty_storage("eve", EVE)
    }

    pub fn new_empty_storage(name: &'static str, id: u8) -> Self {
        Self::new_empty_with(name, id, |_| {})
    }

    pub(crate) fn new_empty_with(name: &'static str, id: u8, f: impl FnOnce(&mut Config)) -> Self {
        Self::new_with(name, id, MockStorage::empty(), f)
    }
}

pub struct Config {
    pub(crate) config: radicle::node::Config,
    pub(crate) local_time: LocalTime,
    pub(crate) policy: SeedingPolicy,
    pub(crate) secret_key: SigningKey,
}

impl Config {
    pub(crate) fn new(id: usize) -> Self {
        let config = radicle::node::Config::test(Alias::from_str("mocky").unwrap());

        Config {
            config,
            local_time: LocalTime::now(),
            policy: SeedingPolicy::default(),
            secret_key: SigningKey::mock(id),
        }
    }
}

impl Peer<Storage> {
    pub fn project(&mut self, name: &str, description: &str) -> RepoId {
        radicle::storage::git::transport::local::register(self.storage().clone());
        let (repo, _) = fixtures::repository(self.tempdir.path().join(name));
        let (rid, _, _) = rad::init(
            &repo,
            name.try_into().unwrap(),
            description,
            radicle::git::fmt::refname!("master"),
            Visibility::default(),
            self.secret_key(),
            self.storage(),
        )
        .unwrap();

        rid
    }
}

impl<S> Peer<S>
where
    S: WriteStorage + 'static,
{
    pub fn with_storage(name: &'static str, id: u8, storage: S) -> Self {
        Self::new_with(name, id, storage, |_| {})
    }

    pub(crate) fn new_with(
        name: &'static str,
        id: u8,
        storage: S,
        f: impl FnOnce(&mut Config),
    ) -> Self {
        let mut config = Config::new(id as usize);

        let policies = policy::Store::<policy::store::Write>::memory().unwrap();
        let mut policies = policy::Config::new(config.policy, policies);
        let ip = [198, 18, 0, id].into();

        let addr = net::SocketAddr::new(ip, 58776 + (id as u16));
        let inventory = storage.repositories().unwrap();

        // Make sure the peer address is advertised.
        config.config.external_addresses.push(addr.into());
        for repo in &inventory {
            policies.seed(&repo.rid, Scope::Followed).unwrap();
        }

        f(&mut config);

        let tempdir = tempfile::TempDir::with_prefix(name).unwrap();

        let nid = *config.secret_key.public_key();

        // Initialize database.
        let db = Database::open(
            tempdir.path().join(node::NODE_DB_FILE),
            node::db::config::Config::default(),
        )
        .unwrap()
        .init(
            &nid,
            config.config.features(),
            &config.config.alias,
            &UserAgent::default(),
            config.local_time.into(),
            config.config.external_addresses.iter(),
        )
        .unwrap()
        .into();

        let announcement =
            service::gossip::node(&config.config, Timestamp::from(config.local_time) + 1);
        let emitter: Emitter<Event> = Default::default();

        let mut service = Service::new(
            config.config,
            db,
            storage,
            policies,
            config.secret_key,
            fastrand::Rng::with_seed(id as u64),
            announcement,
            emitter,
        );

        info!(
            target: "test",
            "{}: Initializing: id = {}, address = {}",
            name, nid, addr
        );

        service.initialize(config.local_time).unwrap();

        Self {
            name,
            service,
            addr,
            tempdir,
        }
    }

    pub fn restart(&mut self) {
        info!(
            target: "test",
            "{}: Restarting: id = {}, address = {}",
            self.name, *self.nid(), self.address()
        );
        self.service.initialize(*self.service.clock()).unwrap();
    }

    pub fn import_addresses<'a>(&mut self, peers: impl IntoIterator<Item = &'a Self>) {
        let timestamp = Timestamp::from(*self.clock());
        for peer in peers.into_iter() {
            let known_address = node::KnownAddress::new(peer.address(), address::Source::Peer);
            self.service
                .database_mut()
                .addresses_mut()
                .insert(
                    peer.nid(),
                    PROTOCOL_VERSION,
                    radicle::node::Features::default(),
                    &Alias::from_str(peer.name).unwrap(),
                    0,
                    &UserAgent::default(),
                    timestamp,
                    Some(known_address),
                )
                .unwrap();
        }
    }

    pub fn inventory(&self) -> HashSet<RepoId> {
        self.service
            .database()
            .routing()
            .get_inventory(self.nid())
            .unwrap()
    }

    pub fn receive(&mut self, peer: NodeId, msg: Message) -> &mut Self {
        self.service.received_message(peer, msg);
        self
    }

    pub fn inventory_announcement(&self) -> Message {
        Message::inventory(
            InventoryAnnouncement {
                inventory: arbitrary::vec(3).try_into().unwrap(),
                timestamp: Timestamp::from(*self.clock()),
            },
            self.secret_key(),
        )
    }

    pub fn node_announcement(&self) -> Message {
        Message::node(
            NodeAnnouncement {
                version: PROTOCOL_VERSION,
                features: node::Features::SEED,
                timestamp: Timestamp::from(*self.clock()),
                alias: Alias::from_str(self.name).unwrap(),
                addresses: Some(self.address()).into(),
                nonce: 0,
                agent: UserAgent::test(),
            }
            .solve(0)
            .unwrap(),
            self.secret_key(),
        )
    }

    pub fn refs_announcement(&self, rid: RepoId) -> Message {
        let mut refs = BoundedVec::new();
        if let Ok(repo) = self.storage().repository(rid)
            && let Ok(false) = repo.is_empty()
            && let Ok(remotes) = repo.remotes()
        {
            for (remote_id, _) in remotes.into_iter() {
                match RefsAt::new(&repo, remote_id) {
                    Ok(refs_at) => {
                        if let Err(e) = refs.push(refs_at) {
                            debug!(target: "test", "Failed to push {remote_id} to refs: {e}");
                            break;
                        }
                    }
                    Err(e) => {
                        debug!(target: "test", "Failed to get `rad/sigrefs` for {remote_id}: {e}")
                    }
                }
            }
        }

        self.announcement(RefsAnnouncement {
            rid,
            refs,
            timestamp: Timestamp::from(*self.clock()),
        })
    }

    pub fn announcement(&self, ann: impl Into<AnnouncementMessage>) -> Message {
        ann.into().signed(self.secret_key()).into()
    }

    pub fn signed_refs_at(&self, root: Oid) -> SignedRefs {
        arbitrary::with_gen(8, |g| {
            refs::arbitrary::signed_refs_at(g, root, self.secret_key())
        })
    }

    pub fn connect_from(&mut self, peer: &Self) {
        let remote_id = *peer.nid();

        self.service
            .connected(remote_id, peer.address(), Link::Inbound);
        self.service
            .received_message(remote_id, peer.node_announcement());

        let mut msgs = self.messages(remote_id);
        msgs.find(|m| {
            matches!(
                m,
                Message::Announcement(Announcement {
                    message: AnnouncementMessage::Inventory(_),
                    ..
                })
            )
        })
        .expect("`inventory-announcement` must be sent");
    }

    pub fn connect_to<T: WriteStorage + 'static>(&mut self, peer: &Peer<T>) {
        let remote_id = *peer.nid();
        let remote_addr = peer.address();

        self.service.command(Command::Connect(
            remote_id,
            remote_addr.clone(),
            ConnectOptions::default(),
        ));

        self.outbox()
            .find(|o| matches!(o, Io::Connect { .. }))
            .unwrap();

        self.service.attempted(remote_id, remote_addr.clone());
        self.service
            .connected(remote_id, remote_addr, Link::Outbound);
        self.service
            .received_message(remote_id, peer.node_announcement());

        let mut msgs = self.messages(remote_id);
        msgs.find(|m| {
            matches!(
                m,
                Message::Announcement(Announcement {
                    message: AnnouncementMessage::Inventory(_),
                    ..
                })
            )
        })
        .expect("`inventory-announcement` must be sent");
    }

    pub fn elapse(&mut self, duration: LocalDuration) {
        self.clock_mut().elapse(duration);
        self.service.wake();
    }

    /// Drain outgoing messages sent from this peer to the remote peer.
    pub fn messages(&mut self, remote: NodeId) -> impl Iterator<Item = Message> + use<S> {
        let mut msgs = Vec::new();

        Service::outbox(&mut self.service)
            .queue()
            .retain(|o| match o {
                Io::Write(a, messages) if *a == remote => {
                    msgs.extend(messages.clone());
                    false
                }
                _ => true,
            });

        msgs.into_iter()
    }

    /// Drain outgoing *relayed* announcements to the remote peer. This doesn't include messages
    /// originating from our own node.
    pub fn relayed(&mut self, remote: NodeId) -> impl Iterator<Item = Message> {
        let mut filtered: Vec<Message> = Vec::new();
        let nid = *self.nid();

        for o in Service::outbox(&mut self.service).queue() {
            match o {
                Io::Write(a, messages) if *a == remote => {
                    let (relayed, other): (Vec<Message>, _) =
                        messages.iter().cloned().partition(|m| {
                            matches!(
                                m,
                                Message::Announcement(Announcement { node, .. })
                                if *node != nid
                            )
                        });
                    *messages = other;
                    filtered.extend(relayed);
                }
                _ => {}
            }
        }

        filtered.into_iter()
    }

    /// Drain outgoing inventories sent from this peer to the remote peer.
    pub fn inventory_announcements(&mut self, remote: NodeId) -> impl Iterator<Item = Message> {
        let mut invs: Vec<Message> = Vec::new();

        for o in Service::outbox(&mut self.service).queue() {
            match o {
                Io::Write(a, messages) if *a == remote => {
                    let (inventories, other): (Vec<Message>, _) =
                        messages.iter().cloned().partition(|m| {
                            MessageType::try_from(m.type_id())
                                == Ok(MessageType::InventoryAnnouncement)
                        });
                    *messages = other;
                    invs.extend(inventories);
                }
                _ => {}
            }
        }

        invs.into_iter()
    }

    /// Get a stream of the peer's emitted events.
    pub fn events(&mut self) -> Events {
        self.service.events()
    }

    /// Get a draining iterator over the peer's I/O outbox.
    pub fn outbox(&mut self) -> impl Iterator<Item = Io> + '_ {
        iter::from_fn(|| Service::outbox(&mut self.service).next())
    }

    /// Get a draining iterator over the peer's I/O outbox, which only returns fetches.
    pub fn fetches(&mut self) -> impl Iterator<Item = (RepoId, NodeId)> + '_ {
        iter::from_fn(|| Service::outbox(&mut self.service).next()).filter_map(|io| {
            if let Io::Fetch { rid, remote, .. } = io {
                Some((rid, remote))
            } else {
                None
            }
        })
    }
}
