#![allow(dead_code)]
use std::iter;
use std::net;
use std::ops::{Deref, DerefMut};

use log::*;

use crate::address;
use crate::address::Store;
use crate::clock::Timestamp;
use crate::crypto::test::signer::MockSigner;
use crate::crypto::Signer;
use crate::identity::Id;
use crate::node;
use crate::prelude::*;
use crate::service;
use crate::service::message::*;
use crate::service::reactor::Io;
use crate::service::*;
use crate::storage::git::transport::remote;
use crate::storage::{RemoteId, WriteStorage};
use crate::test::arbitrary;
use crate::test::simulator;
use crate::test::storage::MockStorage;
use crate::Link;
use crate::{LocalDuration, LocalTime};

/// Service instantiation used for testing.
pub type Service<S, G> = service::Service<routing::Table, address::Book, S, G>;

#[derive(Debug)]
pub struct Peer<S, G> {
    pub name: &'static str,
    pub service: Service<S, G>,
    pub id: NodeId,
    pub ip: net::IpAddr,
    pub rng: fastrand::Rng,
    pub local_addr: net::SocketAddr,

    initialized: bool,
}

impl<S, G> simulator::Peer<S, G> for Peer<S, G>
where
    S: WriteStorage + 'static,
    G: Signer + 'static,
{
    fn init(&mut self) {
        self.initialize()
    }

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
        Self::config(name, ip, MockStorage::empty(), Config::default())
    }
}

pub struct Config<G: Signer + 'static> {
    pub config: service::Config,
    pub addrs: address::Book,
    pub local_time: LocalTime,
    pub signer: G,
    pub rng: fastrand::Rng,
}

impl Default for Config<MockSigner> {
    fn default() -> Self {
        let mut rng = fastrand::Rng::new();
        let signer = MockSigner::new(&mut rng);

        Config {
            config: service::Config::default(),
            addrs: address::Book::memory().unwrap(),
            local_time: LocalTime::now(),
            signer,
            rng,
        }
    }
}

impl<S, G> Peer<S, G>
where
    S: WriteStorage + 'static,
    G: Signer + 'static,
{
    pub fn config(
        name: &'static str,
        ip: impl Into<net::IpAddr>,
        storage: S,
        config: Config<G>,
    ) -> Self {
        let routing = routing::Table::memory().unwrap();
        let tracking = tracking::Config::memory().unwrap();
        let id = *config.signer.public_key();
        let service = Service::new(
            config.config,
            config.local_time,
            routing,
            storage,
            config.addrs,
            tracking,
            config.signer,
            config.rng.clone(),
        );
        let ip = ip.into();
        let local_addr = net::SocketAddr::new(ip, config.rng.u16(..));

        Self {
            name,
            service,
            id,
            ip,
            local_addr,
            rng: config.rng,
            initialized: false,
        }
    }

    pub fn initialize(&mut self) {
        if !self.initialized {
            info!(
                "{}: Initializing: id = {}, address = {}",
                self.name, self.id, self.ip
            );

            self.initialized = true;
            self.service.initialize(LocalTime::now()).unwrap();
        }
    }

    pub fn address(&self) -> Address {
        Address::from(net::SocketAddr::from((self.ip, 8776)))
    }

    pub fn import_addresses<P>(&mut self, peers: P)
    where
        P: AsRef<[Self]>,
    {
        let timestamp = self.timestamp();
        for peer in peers.as_ref() {
            let known_address = address::KnownAddress::new(peer.address(), address::Source::Peer);
            self.service
                .addresses_mut()
                .insert(
                    &peer.node_id(),
                    radicle::node::Features::default(),
                    peer.name,
                    timestamp,
                    Some(known_address),
                )
                .unwrap();
        }
    }

    pub fn timestamp(&self) -> Timestamp {
        self.clock().as_secs()
    }

    pub fn git_url(&self, repo: Id, namespace: Option<RemoteId>) -> remote::Url {
        remote::Url {
            node: self.node_id(),
            repo,
            namespace,
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.service.node_id()
    }

    pub fn receive(&mut self, peer: NodeId, msg: Message) {
        self.service.received_message(peer, msg);
    }

    pub fn inventory_announcement(&self) -> Message {
        Message::inventory(
            InventoryAnnouncement {
                inventory: arbitrary::gen(3),
                timestamp: self.timestamp(),
            },
            self.signer(),
        )
    }

    pub fn node_announcement(&self) -> Message {
        let mut alias = [0u8; 32];
        alias[..self.name.len()].copy_from_slice(self.name.as_bytes());

        Message::node(
            NodeAnnouncement {
                features: node::Features::SEED,
                timestamp: self.timestamp(),
                alias,
                addresses: Some(net::SocketAddr::from((self.ip, node::DEFAULT_PORT)).into()).into(),
                nonce: 0,
            }
            .solve(),
            self.signer(),
        )
    }

    pub fn refs_announcement(&self, id: Id) -> Message {
        let refs = BoundedVec::new();
        let ann = AnnouncementMessage::from(RefsAnnouncement {
            id,
            refs,
            timestamp: self.timestamp(),
        });
        let msg = ann.signed(self.signer());

        msg.into()
    }

    pub fn connect_from(&mut self, peer: &Self) {
        let remote_id = simulator::Peer::<S, G>::id(peer);

        self.initialize();
        self.service.connected(remote_id, Link::Inbound);
        self.receive(remote_id, Message::init());

        let mut msgs = self.messages(remote_id);
        msgs.find(|m| matches!(m, Message::Initialize { .. }))
            .expect("`initialize` must be sent");
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

    pub fn connect_to(&mut self, peer: &Self) {
        let remote_id = simulator::Peer::<S, G>::id(peer);
        let remote_addr = simulator::Peer::<S, G>::addr(peer);

        self.initialize();
        self.service.attempted(remote_id, &remote_addr);
        self.service.connected(remote_id, Link::Outbound);

        let mut msgs = self.messages(remote_id);
        msgs.find(|m| matches!(m, Message::Initialize { .. }))
            .expect("`initialize` must be sent");
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

        self.receive(remote_id, Message::init());
    }

    pub fn elapse(&mut self, duration: LocalDuration) {
        self.clock_mut().elapse(duration);
        self.service.wake();
    }

    /// Drain outgoing messages sent from this peer to the remote address.
    pub fn messages(&mut self, remote: NodeId) -> impl Iterator<Item = Message> {
        let mut msgs = Vec::new();

        self.service.reactor().outbox().retain(|o| match o {
            Io::Write(a, messages) if *a == remote => {
                msgs.extend(messages.clone());
                false
            }
            _ => true,
        });

        msgs.into_iter()
    }

    /// Get a draining iterator over the peer's emitted events.
    pub fn events(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.outbox()
            .filter_map(|io| if let Io::Event(e) = io { Some(e) } else { None })
    }

    /// Get a draining iterator over the peer's I/O outbox.
    pub fn outbox(&mut self) -> impl Iterator<Item = Io> + '_ {
        iter::from_fn(|| self.service.reactor().next())
    }
}
