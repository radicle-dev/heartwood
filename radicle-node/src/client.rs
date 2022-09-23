use std::net;

use crossbeam_channel as chan;
use nakamoto_net::{LocalTime, Reactor};

use crate::clock::RefClock;
use crate::collections::HashMap;
use crate::profile::Profile;
use crate::service;
use crate::transport::Transport;
use crate::wire::Wire;

pub mod handle;

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
    profile: Profile,

    handle: chan::Sender<service::Command>,
    commands: chan::Receiver<service::Command>,
    shutdown: chan::Sender<()>,
    listening: chan::Receiver<net::SocketAddr>,
    events: Events,
}

impl<R: Reactor> Client<R> {
    pub fn new(profile: Profile) -> Result<Self, nakamoto_net::error::Error> {
        let (handle, commands) = chan::unbounded::<service::Command>();
        let (shutdown, shutdown_recv) = chan::bounded(1);
        let (listening_send, listening) = chan::bounded(1);
        let reactor = R::new(shutdown_recv, listening_send)?;
        let events = Events {};

        Ok(Self {
            profile,
            reactor,
            handle,
            commands,
            listening,
            shutdown,
            events,
        })
    }

    pub fn run(mut self, config: Config) -> Result<(), nakamoto_net::error::Error> {
        let network = config.service.network;
        let rng = fastrand::Rng::new();
        let time = LocalTime::now();
        let storage = self.profile.storage;
        let signer = self.profile.signer;
        let addresses = HashMap::with_hasher(rng.clone().into());

        log::info!("Initializing client ({:?})..", network);

        let service = service::Service::new(
            config.service,
            RefClock::from(time),
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
