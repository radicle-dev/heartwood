use std::net;
use std::path::Path;

use crossbeam_channel as chan;
use nakamoto_net::{LocalTime, Reactor};

use crate::clock::RefClock;
use crate::collections::HashMap;
use crate::crypto::Signer;
use crate::protocol;
use crate::storage::git::Storage;

pub mod handle;

/// Client configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Client protocol configuration.
    pub protocol: protocol::Config,
    /// Client listen addresses.
    pub listen: Vec<net::SocketAddr>,
}

impl Config {
    /// Create a new configuration for the given network.
    pub fn new(network: protocol::Network) -> Self {
        Self {
            protocol: protocol::Config {
                network,
                ..protocol::Config::default()
            },
            ..Self::default()
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            protocol: protocol::Config::default(),
            listen: vec![([0, 0, 0, 0], 0).into()],
        }
    }
}

pub struct Client<R: Reactor> {
    reactor: R,
    storage: Storage,

    handle: chan::Sender<protocol::Command>,
    commands: chan::Receiver<protocol::Command>,
    shutdown: chan::Sender<()>,
    listening: chan::Receiver<net::SocketAddr>,
    events: Events,
}

impl<R: Reactor> Client<R> {
    pub fn new<P: AsRef<Path>, S: Signer + 'static>(
        path: P,
        signer: S,
    ) -> Result<Self, nakamoto_net::error::Error> {
        let (handle, commands) = chan::unbounded::<protocol::Command>();
        let (shutdown, shutdown_recv) = chan::bounded(1);
        let (listening_send, listening) = chan::bounded(1);
        let reactor = R::new(shutdown_recv, listening_send)?;
        let storage = Storage::open(path, signer)?;
        let events = Events {};

        Ok(Self {
            storage,
            reactor,
            handle,
            commands,
            listening,
            shutdown,
            events,
        })
    }

    pub fn run(mut self, config: Config) -> Result<(), nakamoto_net::error::Error> {
        let network = config.protocol.network;
        let rng = fastrand::Rng::new();
        let time = LocalTime::now();
        let storage = self.storage;
        let signer = storage.signer();
        let addresses = HashMap::with_hasher(rng.clone().into());

        log::info!("Initializing client ({:?})..", network);

        self.reactor.run(
            &config.listen,
            protocol::Protocol::new(
                config.protocol,
                RefClock::from(time),
                storage,
                addresses,
                signer,
                rng,
            ),
            self.events,
            self.commands,
        )?;

        Ok(())
    }

    /// Create a new handle to communicate with the client.
    pub fn handle(&self) -> handle::Handle<R> {
        handle::Handle {
            waker: self.reactor.waker(),
            commands: self.handle.clone(),
            shutdown: self.shutdown.clone(),
            listening: self.listening.clone(),
        }
    }
}

pub struct Events {}

impl nakamoto_net::Publisher<protocol::Event> for Events {
    fn publish(&mut self, e: protocol::Event) {
        log::info!("Received event {:?}", e);
    }
}
