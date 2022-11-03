use std::{io, net};

use crossbeam_channel as chan;
use nakamoto_net::{LocalTime, Reactor};
use thiserror::Error;

use radicle::crypto::Signer;

use crate::clock::RefClock;
use crate::profile::Profile;
use crate::service::routing;
use crate::transport::Transport;
use crate::wire::Wire;
use crate::{address, service};

pub mod handle;

/// Directory in `$RAD_HOME` under which node-specific files are stored.
pub const NODE_DIR: &str = "node";
/// Filename of routing table database under [`NODE_DIR`].
pub const ROUTING_DB_FILE: &str = "routing.db";
/// Filename of address database under [`NODE_DIR`].
pub const ADDRESS_DB_FILE: &str = "addresses.db";

/// A client error.
#[derive(Error, Debug)]
pub enum Error {
    /// A routing database error.
    #[error("routing database error: {0}")]
    Routing(#[from] routing::Error),
    /// An address database error.
    #[error("address database error: {0}")]
    Addresses(#[from] address::Error),
    /// An I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    /// A networking error.
    #[error("network error: {0}")]
    Net(#[from] nakamoto_net::error::Error),
}

/// Client configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Client service configuration.
    pub service: service::Config,
    /// Client listen addresses.
    pub listen: Vec<net::SocketAddr>,
}

impl Config {
    /// Create a new configuration for the given network.
    pub fn new(network: service::Network) -> Self {
        Self {
            service: service::Config {
                network,
                ..service::Config::default()
            },
            ..Self::default()
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            service: service::Config::default(),
            listen: vec![([0, 0, 0, 0], 0).into()],
        }
    }
}

pub struct Client<R: Reactor> {
    reactor: R,

    handle: chan::Sender<service::Command>,
    commands: chan::Receiver<service::Command>,
    shutdown: chan::Sender<()>,
    listening: chan::Receiver<net::SocketAddr>,
    events: Events,
}

impl<R: Reactor> Client<R> {
    pub fn new() -> Result<Self, Error> {
        let (handle, commands) = chan::unbounded::<service::Command>();
        let (shutdown, shutdown_recv) = chan::bounded(1);
        let (listening_send, listening) = chan::bounded(1);
        let reactor = R::new(shutdown_recv, listening_send)?;
        let events = Events {};

        Ok(Self {
            reactor,
            handle,
            commands,
            listening,
            shutdown,
            events,
        })
    }

    pub fn run<G: Signer>(
        mut self,
        config: Config,
        profile: Profile,
        signer: G,
    ) -> Result<(), Error> {
        let network = config.service.network;
        let rng = fastrand::Rng::new();
        let time = LocalTime::now();
        let storage = profile.storage;
        let node_dir = profile.home.join(NODE_DIR);
        let address_db = node_dir.join(ADDRESS_DB_FILE);
        let routing_db = node_dir.join(ROUTING_DB_FILE);

        log::info!("Opening address book {}..", address_db.display());
        let addresses = address::Book::open(address_db)?;

        log::info!("Opening routing table {}..", routing_db.display());
        let routing = routing::Table::open(routing_db)?;

        log::info!("Initializing client ({:?})..", network);

        let service = service::Service::new(
            config.service,
            RefClock::from(time),
            routing,
            storage,
            addresses,
            signer,
            rng,
        );

        self.reactor.run(
            &config.listen,
            Transport::new(Wire::new(service)),
            self.events,
            self.commands,
        )?;

        Ok(())
    }

    /// Create a new handle to communicate with the client.
    pub fn handle(&self) -> handle::Handle<R::Waker> {
        handle::Handle {
            waker: self.reactor.waker(),
            commands: self.handle.clone(),
            shutdown: self.shutdown.clone(),
            listening: self.listening.clone(),
        }
    }
}

pub struct Events {}

impl nakamoto_net::Publisher<service::Event> for Events {
    fn publish(&mut self, e: service::Event) {
        log::info!("Received event {:?}", e);
    }
}
