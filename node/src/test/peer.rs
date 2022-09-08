use std::net;
use std::ops::{Deref, DerefMut};

use git_url::Url;
use log::*;
use nakamoto_net::simulator;
use nakamoto_net::Protocol as _;

use crate::address_book::{KnownAddress, Source};
use crate::clock::RefClock;
use crate::collections::HashMap;
use crate::decoder::Decoder;
use crate::protocol::config::*;
use crate::protocol::message::*;
use crate::protocol::*;
use crate::storage::WriteStorage;
use crate::test::crypto::MockSigner;
use crate::*;

/// Protocol instantiation used for testing.
pub type Protocol<S> = crate::protocol::Protocol<HashMap<net::IpAddr, KnownAddress>, S, MockSigner>;

#[derive(Debug)]
pub struct Peer<S> {
    pub name: &'static str,
    pub protocol: Protocol<S>,
    pub ip: net::IpAddr,
    pub rng: fastrand::Rng,
    pub local_time: LocalTime,
    pub local_addr: net::SocketAddr,

    initialized: bool,
}

impl<'r, S> simulator::Peer<Protocol<S>> for Peer<S>
where
    S: WriteStorage<'r> + 'static,
{
    fn init(&mut self) {
        self.initialize()
    }

    fn addr(&self) -> net::SocketAddr {
        net::SocketAddr::new(self.ip, DEFAULT_PORT)
    }
}

impl<S> Deref for Peer<S> {
    type Target = Protocol<S>;

    fn deref(&self) -> &Self::Target {
        &self.protocol
    }
}

impl<S> DerefMut for Peer<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.protocol
    }
}

impl<'r, S> Peer<S>
where
    S: WriteStorage<'r> + 'static,
{
    pub fn new(name: &'static str, ip: impl Into<net::IpAddr>, storage: S) -> Self {
        Self::config(
            name,
            Config {
                git_url: storage.url(),
                ..Config::default()
            },
            ip,
            vec![],
            storage,
            fastrand::Rng::new(),
        )
    }

    pub fn config(
        name: &'static str,
        config: Config,
        ip: impl Into<net::IpAddr>,
        addrs: Vec<(net::SocketAddr, Source)>,
        storage: S,
        mut rng: fastrand::Rng,
    ) -> Self {
        let addrs = addrs
            .into_iter()
            .map(|(addr, src)| (addr.ip(), KnownAddress::new(addr, src, None)))
            .collect();
        let local_time = LocalTime::now();
        let clock = RefClock::from(local_time);
        let signer = MockSigner::new(&mut rng);
        let protocol = Protocol::new(config, clock, storage, addrs, signer, rng.clone());
        let ip = ip.into();
        let local_addr = net::SocketAddr::new(ip, rng.u16(..));

        Self {
            name,
            protocol,
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
            self.protocol.initialize(LocalTime::now());
        }
    }

    pub fn timestamp(&self) -> Timestamp {
        self.protocol.timestamp()
    }

    pub fn git_url(&self) -> Url {
        self.config().git_url.clone()
    }

    pub fn id(&self) -> NodeId {
        self.protocol.id()
    }

    pub fn receive(&mut self, peer: &net::SocketAddr, msg: Message) {
        let bytes = wire::serialize(&self.config().network.envelope(msg));

        self.protocol.received_bytes(peer, &bytes);
    }

    pub fn connect_from(&mut self, peer: &Self) {
        let remote = simulator::Peer::<Protocol<S>>::addr(peer);
        let local = net::SocketAddr::new(self.ip, self.rng.u16(..));
        let git = format!("file:///{}.git", remote.ip());
        let git = Url::from_bytes(git.as_bytes()).unwrap();

        self.initialize();
        self.protocol.connected(remote, &local, Link::Inbound);
        self.receive(
            &remote,
            Message::hello(
                peer.id(),
                self.local_time().as_secs(),
                vec![Address::from(remote)],
                git,
            ),
        );

        let mut msgs = self.messages(&remote);
        msgs.find(|m| matches!(m, Message::Hello { .. }))
            .expect("`hello` is sent");
        msgs.find(|m| matches!(m, Message::GetInventory { .. }))
            .expect("`get-inventory` is sent");
    }

    pub fn connect_to(&mut self, peer: &Self) {
        let remote = simulator::Peer::<Protocol<S>>::addr(peer);

        self.initialize();
        self.protocol.attempted(&remote);
        self.protocol
            .connected(remote, &self.local_addr, Link::Outbound);

        let mut msgs = self.messages(&remote);
        msgs.find(|m| matches!(m, Message::Hello { .. }))
            .expect("`hello` is sent");
        msgs.find(|m| matches!(m, Message::GetInventory { .. }))
            .expect("`get-inventory` is sent");

        let git = peer.config().git_url.clone();
        self.receive(
            &remote,
            Message::hello(
                peer.id(),
                self.local_time().as_secs(),
                peer.config().listen.clone(),
                git,
            ),
        );
    }

    /// Drain outgoing messages sent from this peer to the remote address.
    pub fn messages(&mut self, remote: &net::SocketAddr) -> impl Iterator<Item = Message> {
        let mut stream = Decoder::<Envelope>::new(2048);
        let mut msgs = Vec::new();

        self.protocol.outbox().retain(|o| match o {
            Io::Write(a, bytes) if a == remote => {
                stream.input(bytes);
                false
            }
            _ => true,
        });

        while let Some(envelope) = stream.decode_next().unwrap() {
            msgs.push(envelope.msg);
        }
        msgs.into_iter()
    }

    /// Get a draining iterator over the peers's I/O outbox.
    pub fn outbox(&mut self) -> impl Iterator<Item = Io<Event, DisconnectReason>> + '_ {
        self.protocol.outbox().drain(..)
    }
}
