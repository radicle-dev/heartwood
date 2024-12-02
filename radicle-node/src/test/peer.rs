#![allow(dead_code)]
use std::collections::HashSet;
use std::iter;
use std::net;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use log::*;

use radicle::crypto;
use radicle::identity::Visibility;
use radicle::node::address::Store as _;
use radicle::node::device::Device;
use radicle::node::Database;
use radicle::node::UserAgent;
use radicle::node::{address, Alias, ConnectOptions};
use radicle::rad;
use radicle::storage::refs::{RefsAt, SignedRefsAt, IDENTITY_ROOT};
use radicle::storage::{ReadRepository, RemoteRepository};
use radicle::Storage;

use crate::crypto::test::signer::MockSigner;
use crate::identity::RepoId;
use crate::node;
use crate::node::routing::Store as _;
use crate::prelude::*;
use crate::runtime::Emitter;
use crate::service;
use crate::service::io::Io;
use crate::service::message::*;
use crate::service::policy::{Scope, SeedingPolicy};
use crate::service::*;
use crate::storage::git::transport::remote;
use crate::storage::{RemoteId, WriteStorage};
use crate::test::storage::MockStorage;
use crate::test::{arbitrary, fixtures, simulator};
use crate::wire::MessageType;
use crate::{Link, LocalDuration, LocalTime, PROTOCOL_VERSION};

/// Service instantiation used for testing.
pub type Service<S, G> = service::Service<Database, S, G>;

#[derive(Debug)]
pub struct Peer<S, G> {
    pub name: &'static str,
    pub service: Service<S, G>,
    pub id: NodeId,
    pub ip: net::IpAddr,
    pub local_time: LocalTime,
    pub rng: fastrand::Rng,
    pub local_addr: net::SocketAddr,
    pub tempdir: tempfile::TempDir,

    initialized: bool,
}

impl<S, G> simulator::Peer<S, G> for Peer<S, G>
where
    S: WriteStorage + 'static,
    G: crypto::signature::Signer<crypto::Signature> + 'static,
{
    fn init(&mut self) {}

    fn addr(&self) -> Address {
        self.address()
    }

    fn id(&self) -> NodeId {
        self.id
    }
}

impl<S, G> Deref for Peer<S, G> {
    type Target = Service<S, G>;

    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

impl<S, G> DerefMut for Peer<S, G> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.service
    }
}

impl Peer<MockStorage, MockSigner> {
    pub fn new(name: &'static str, ip: impl Into<net::IpAddr>) -> Self {
        Self::with_storage(name, ip, MockStorage::empty()).initialized()
    }
}

impl<S> Peer<S, MockSigner>
where
    S: WriteStorage + 'static,
{
    pub fn with_storage(name: &'static str, ip: impl Into<net::IpAddr>, storage: S) -> Self {
        Self::config(name, ip, storage, Config::default()).initialized()
    }
}

pub struct Config<G: crypto::signature::Signer<crypto::Signature> + 'static> {
    pub config: service::Config,
    pub local_time: LocalTime,
    pub policy: SeedingPolicy,
    pub signer: Device<G>,
    pub rng: fastrand::Rng,
    pub tmp: tempfile::TempDir,
}

impl Default for Config<MockSigner> {
    fn default() -> Self {
        let mut rng = fastrand::Rng::new();
        let signer = Device::mock_rng(&mut rng);
        let tmp = tempfile::TempDir::new().unwrap();
        let config = service::Config::test(Alias::from_str("mocky").unwrap());

        Config {
            config,
            local_time: LocalTime::now(),
            policy: SeedingPolicy::default(),
            signer,
            rng,
            tmp,
        }
    }
}

impl<G: crypto::signature::Signer<crypto::Signature>> Peer<Storage, G> {
    pub fn project(&mut self, name: &str, description: &str) -> RepoId {
        radicle::storage::git::transport::local::register(self.storage().clone());

        let (repo, _) = fixtures::repository(self.tempdir.path().join(name));
        let (rid, _, _) = rad::init(
            &repo,
            name.try_into().unwrap(),
            description,
            radicle::git::refname!("master"),
            Visibility::default(),
            self.signer(),
            self.storage(),
        )
        .unwrap();

        rid
    }
}

impl<S, G> Peer<S, G>
where
    S: WriteStorage + 'static,
    G: crypto::signature::Signer<crypto::Signature> + 'static,
{
    pub fn config(
        name: &'static str,
        ip: impl Into<net::IpAddr>,
        storage: S,
        mut config: Config<G>,
    ) -> Self {
        let policies = policy::Store::<policy::store::Write>::memory().unwrap();
        let mut policies = policy::Config::new(config.policy, policies);
        let id = *config.signer.public_key();
        let ip = ip.into();
        let local_addr = net::SocketAddr::new(ip, config.rng.u16(..));
        let inventory = storage.repositories().unwrap();

        // Make sure the peer address is advertized.
        config.config.external_addresses.push(local_addr.into());
        for repo in &inventory {
            policies.seed(&repo.rid, Scope::Followed).unwrap();
        }
        // Initialize database.
        let db = Database::open(config.tmp.path().join(node::NODE_DB_FILE))
            .unwrap()
            .init(
                &id,
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
        let service = Service::new(
            config.config,
            db,
            storage,
            policies,
            config.signer,
            config.rng.clone(),
            announcement,
            emitter,
        );

        Self {
            name,
            service,
            id,
            ip,
            local_addr,
            local_time: config.local_time,
            rng: config.rng,
            initialized: false,
            tempdir: config.tmp,
        }
    }

    pub fn initialize(&mut self) -> &mut Self {
        if !self.initialized {
            info!(
                target: "test",
                "{}: Initializing: id = {}, address = {}",
                self.name, self.id, self.ip
            );
        }
        assert_ne!(self.local_time, LocalTime::default());

        self.initialized = true;
        self.service.initialize(self.local_time).unwrap();
        self
    }

    pub fn initialized(mut self) -> Self {
        self.initialize();
        self
    }

    pub fn restart(&mut self) {
        assert!(self.initialized);
        info!(
            target: "test",
            "{}: Restarting: id = {}, address = {}",
            self.name, self.id, self.ip
        );
        self.service.initialize(*self.service.clock()).unwrap();
    }

    pub fn address(&self) -> Address {
        Address::from(net::SocketAddr::from((self.ip, 8776)))
    }

    pub fn import_addresses<'a>(&mut self, peers: impl IntoIterator<Item = &'a Self>) {
        let timestamp = self.timestamp();
        for peer in peers.into_iter() {
            let known_address = node::KnownAddress::new(peer.address(), address::Source::Peer);
            self.service
                .database_mut()
                .addresses_mut()
                .insert(
                    &peer.node_id(),
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

    pub fn timestamp(&self) -> Timestamp {
        (*self.clock()).into()
    }

    pub fn inventory(&self) -> HashSet<RepoId> {
        self.service
            .database()
            .routing()
            .get_inventory(self.nid())
            .unwrap()
    }

    pub fn git_url(&self, repo: RepoId, namespace: Option<RemoteId>) -> remote::Url {
        remote::Url {
            node: self.node_id(),
            repo,
            namespace,
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.service.node_id()
    }

    pub fn receive(&mut self, peer: NodeId, msg: Message) -> &mut Self {
        self.service.received_message(peer, msg);
        self
    }

    pub fn inventory_announcement(&self) -> Message {
        Message::inventory(
            InventoryAnnouncement {
                inventory: arbitrary::vec(3).try_into().unwrap(),
                timestamp: self.timestamp(),
            },
            self.signer(),
        )
    }

    pub fn node_announcement(&self) -> Message {
        Message::node(
            NodeAnnouncement {
                version: PROTOCOL_VERSION,
                features: node::Features::SEED,
                timestamp: self.timestamp(),
                alias: Alias::from_str(self.name).unwrap(),
                addresses: Some(net::SocketAddr::from((self.ip, node::DEFAULT_PORT)).into()).into(),
                nonce: 0,
                agent: UserAgent::from_str("/radicle:test/").unwrap(),
            }
            .solve(0)
            .unwrap(),
            self.signer(),
        )
    }

    pub fn refs_announcement(&self, rid: RepoId) -> Message {
        let mut refs = BoundedVec::new();
        if let Ok(repo) = self.storage().repository(rid) {
            if let Ok(false) = repo.is_empty() {
                if let Ok(remotes) = repo.remotes() {
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
            }
        }

        self.announcement(RefsAnnouncement {
            rid,
            refs,
            timestamp: self.timestamp(),
        })
    }

    pub fn announcement(&self, ann: impl Into<AnnouncementMessage>) -> Message {
        ann.into().signed(self.signer()).into()
    }

    pub fn signed_refs_at<R: ReadRepository>(
        &self,
        mut refs: Refs,
        at: radicle::git::Oid,
        repo: &R,
    ) -> SignedRefsAt {
        refs.insert(IDENTITY_ROOT.to_ref_string(), repo.identity_root().unwrap());
        SignedRefsAt {
            sigrefs: refs.signed(self.signer()).unwrap().verified(repo).unwrap(),
            at,
        }
    }

    pub fn connect_from(&mut self, peer: &Self) {
        let remote_id = simulator::Peer::<S, G>::id(peer);

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

    pub fn connect_to<
        T: WriteStorage + 'static,
        H: crypto::signature::Signer<crypto::Signature> + 'static,
    >(
        &mut self,
        peer: &Peer<T, H>,
    ) {
        let remote_id = simulator::Peer::<T, H>::id(peer);
        let remote_addr = simulator::Peer::<T, H>::addr(peer);

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
    pub fn messages(&mut self, remote: NodeId) -> impl Iterator<Item = Message> {
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
