use std::io;
use std::{net, thread, time};

use netservices::wire::NetAccept;
use reactor::poller::popol;
use reactor::Reactor;
use thiserror::Error;

use crate::address;
use crate::control;
use crate::node::NodeId;
use crate::service::{routing, tracking};
use crate::wire::Transport;
use crate::worker::{WorkerPool, WorkerReq};
use crate::{crypto, profile, service, LocalTime};

pub mod handle;
use handle::Handle;

/// Directory in `$RAD_HOME` under which node-specific files are stored.
pub const NODE_DIR: &str = "node";
/// Filename of routing table database under [`NODE_DIR`].
pub const ROUTING_DB_FILE: &str = "routing.db";
/// Filename of address database under [`NODE_DIR`].
pub const ADDRESS_DB_FILE: &str = "addresses.db";
/// Filename of tracking table database under [`NODE_DIR`].
pub const TRACKING_DB_FILE: &str = "tracking.db";

/// A client error.
#[derive(Error, Debug)]
pub enum Error {
    /// A routing database error.
    #[error("routing database error: {0}")]
    Routing(#[from] routing::Error),
    /// An address database error.
    #[error("address database error: {0}")]
    Addresses(#[from] address::Error),
    /// A tracking database error.
    #[error("tracking database error: {0}")]
    Tracking(#[from] tracking::Error),
    /// An I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    /// A networking error.
    #[error("network error: {0}")]
    Net(#[from] nakamoto_net::error::Error),
    /// A control socket error.
    #[error("control socket error: {0}")]
    Control(#[from] control::Error),
}

/// Holds join handles to the client threads, as well as a client handle.
pub struct Runtime<G: crypto::Signer + crypto::Negotiator> {
    pub id: NodeId,
    pub handle: Handle<Transport<routing::Table, address::Book, radicle::Storage, G>>,
    pub control: thread::JoinHandle<Result<(), control::Error>>,
    pub reactor: Reactor<Transport<service::routing::Table, address::Book, radicle::Storage, G>>,
    pub pool: WorkerPool,
    pub local_addrs: Vec<net::SocketAddr>,
}

impl<G: crypto::Signer + crypto::Negotiator + 'static> Runtime<G> {
    /// Run the client.
    ///
    /// This function spawns threads.
    pub fn with(
        profile: profile::Profile,
        config: service::Config,
        listen: Vec<net::SocketAddr>,
        proxy: net::SocketAddr,
        signer: G,
    ) -> Result<Runtime<G>, Error> {
        let id = *profile.id();
        let node = profile.node();
        let negotiator = signer.clone();
        let network = config.network;
        let rng = fastrand::Rng::new();
        let clock = LocalTime::now();
        let storage = profile.storage;
        let node_dir = profile.home.join(NODE_DIR);
        let address_db = node_dir.join(ADDRESS_DB_FILE);
        let routing_db = node_dir.join(ROUTING_DB_FILE);
        let tracking_db = node_dir.join(TRACKING_DB_FILE);

        log::info!("Opening address book {}..", address_db.display());
        let addresses = address::Book::open(address_db)?;

        log::info!("Opening routing table {}..", routing_db.display());
        let routing = routing::Table::open(routing_db)?;

        log::info!("Opening tracking policy table {}..", tracking_db.display());
        let tracking = tracking::Config::open(tracking_db)?;

        log::info!("Initializing service ({:?})..", network);
        let service = service::Service::new(
            config,
            clock,
            routing,
            storage.clone(),
            addresses,
            tracking,
            signer,
            rng,
        );

        let (worker_send, worker_recv) = crossbeam_channel::unbounded::<WorkerReq<G>>();
        let pool = WorkerPool::with(10, time::Duration::from_secs(9), storage, worker_recv);
        let wire = Transport::new(service, worker_send, negotiator.clone(), proxy, clock);
        let reactor = Reactor::new(wire, popol::Poller::new())?;
        let handle = Handle::from(reactor.controller());
        let control = thread::spawn({
            let handle = handle.clone();
            move || control::listen(node, handle)
        });
        let controller = reactor.controller();
        let mut local_addrs = Vec::new();

        for addr in listen {
            let listener = NetAccept::bind(addr, negotiator.clone())?;
            let local_addr = listener.local_addr();

            local_addrs.push(local_addr);
            controller.register_listener(listener)?;

            log::info!("Listening on {local_addr}..");
        }

        Ok(Runtime {
            id,
            control,
            reactor,
            handle,
            pool,
            local_addrs,
        })
    }

    pub fn run(self) -> Result<(), Error> {
        log::info!("Running node {}..", self.id);

        self.pool.run().unwrap();
        self.control.join().unwrap()?;
        self.reactor.join().unwrap();

        Ok(())
    }
}
