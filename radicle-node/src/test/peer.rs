#![allow(dead_code)]
use std::collections::BTreeMap;
use std::iter;
use std::net;
use std::ops::{Deref, DerefMut};

use log::*;

use crate::address;
use crate::address::Store;
use crate::clock::{RefClock, Timestamp};
use crate::crypto::test::signer::MockSigner;
use crate::crypto::Signer;
use crate::identity::Id;
use crate::node;
use crate::prelude::NodeId;
use crate::service;
use crate::service::config::*;
use crate::service::message::*;
use crate::service::reactor::Io;
use crate::service::*;
use crate::storage::git::transport::remote;
use crate::storage::{RemoteId, WriteStorage};
use crate::test::arbitrary;
use crate::test::simulator;
use crate::{Link, LocalDuration, LocalTime};

/// Service instantiation used for testing.
pub type Service<S, G> = service::Service<routing::Table, address::Book, S, G>;

#[derive(Debug)]
pub struct Peer<S, G> {
    pub name: &'static str,
    pub service: Service<S, G>,
    pub ip: net::IpAddr,
    pub rng: fastrand::Rng,
    pub local_time: LocalTime,
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

    fn addr(&self) -> net::SocketAddr {
        net::SocketAddr::new(self.ip, DEFAULT_PORT)
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

impl<S> Peer<S, MockSigner>
where
    S: WriteStorage + 'static,
{
    pub fn new(name: &'static str, ip: impl Into<net::IpAddr>, storage: S) -> Self {
        let mut rng = fastrand::Rng::new();
        let signer = MockSigner::new(&mut rng);

        let addrs = address::Book::memory().unwrap();
        Self::config(name, Config::default(), ip, storage, addrs, signer, rng)
    }
}

impl<S, G> Peer<S, G>
where
    S: WriteStorage + 'static,
    G: Signer + 'static,
{
    pub fn config(
        name: &'static str,
        config: Config,
        ip: impl Into<net::IpAddr>,
        storage: S,
        addrs: address::Book,
        signer: G,
        rng: fastrand::Rng,
    ) -> Self {
        let local_time = LocalTime::now();
        let clock = RefClock::from(local_time);
        let routing = routing::Table::memory().unwrap();
        let service = Service::new(config, clock, routing, storage, addrs, signer, rng.clone());
        let ip = ip.into();
        let local_addr = net::SocketAddr::new(ip, rng.u16(..));

        Self {
            name,
            service,
            ip,
            local_addr,
            rng,
            local_time,
            initialized: false,
        }
    }

    pub fn initialize(&mut self) {
        if !self.initialized {
            info!("{}: Initializing: address = {}", self.name, self.ip);

            self.initialized = true;
            self.service.initialize(LocalTime::now());
        }
    }

    pub fn address(&self) -> Address {
        simulator::Peer::addr(self).into()
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
        self.service.clock().timestamp()
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

    pub fn receive(&mut self, peer: &net::SocketAddr, msg: Message) {
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
                addresses: vec![net::SocketAddr::from((self.ip, service::DEFAULT_PORT)).into()],
                nonce: 0,
            }
            .solve(),
            self.signer(),
        )
    }

    pub fn refs_announcement(&self, id: Id) -> Message {
        let refs = BTreeMap::new().into();
        let ann = AnnouncementMessage::from(RefsAnnouncement {
            id,
            refs,
            timestamp: self.timestamp(),
        });
        let msg = ann.signed(self.signer());

        msg.into()
    }

    pub fn connect_from(&mut self, peer: &Self) {
        let remote = simulator::Peer::<S, G>::addr(peer);
        let local = net::SocketAddr::new(self.ip, self.rng.u16(..));

        self.initialize();
        self.service.connecting(remote, &local, Link::Inbound);
        self.service.connected(remote, Link::Inbound);
        self.receive(
            &remote,
            Message::init(peer.node_id(), vec![Address::from(remote)]),
        );

        let mut msgs = self.messages(&remote);
        msgs.find(|m| matches!(m, Message::Initialize { .. }))
            .expect("`initialize` is sent");
        msgs.find(|m| {
            matches!(
                m,
                Message::Announcement(Announcement {
                    message: AnnouncementMessage::Inventory(_),
                    ..
                })
            )
        })
        .expect("`inventory-announcement` is sent");
    }

    pub fn connect_to(&mut self, peer: &Self) {
        let remote = simulator::Peer::<S, G>::addr(peer);

        self.initialize();
        self.service.attempted(&remote);
        self.service
            .connecting(remote, &self.local_addr, Link::Outbound);
        self.service.connected(remote, Link::Outbound);

        let mut msgs = self.messages(&remote);
        msgs.find(|m| matches!(m, Message::Initialize { .. }))
            .expect("`initialize` is sent");
        msgs.find(|m| {
            matches!(
                m,
                Message::Announcement(Announcement {
                    message: AnnouncementMessage::Inventory(_),
                    ..
                })
            )
        })
        .expect("`inventory-announcement` is sent");

        self.receive(
            &remote,
            Message::init(peer.node_id(), peer.config().listen.clone()),
        );
    }

    pub fn elapse(&mut self, duration: LocalDuration) {
        self.clock().elapse(duration);
        self.service.wake();
    }

    /// Drain outgoing messages sent from this peer to the remote address.
    pub fn messages(&mut self, remote: &net::SocketAddr) -> impl Iterator<Item = Message> {
        let mut msgs = Vec::new();

        self.service.reactor().outbox().retain(|o| match o {
            Io::Write(a, messages) if a == remote => {
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
