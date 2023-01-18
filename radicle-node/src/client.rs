use std::{io, net, thread, time};

use crossbeam_channel as chan;
use cyphernet::{Cert, EcSign};
use netservices::resource::NetAccept;
use radicle::profile::Home;
use radicle::Storage;
use reactor::poller::popol;
use reactor::Reactor;
use thiserror::Error;

use crate::address;
use crate::control;
use crate::crypto::Signature;
use crate::node::NodeId;
use crate::service::{routing, tracking};
use crate::wire;
use crate::wire::Wire;
use crate::worker::{WorkerPool, WorkerReq};
use crate::{crypto, service, LocalTime};

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
pub struct Runtime {
    pub id: NodeId,
    pub handle: Handle,
    pub control: thread::JoinHandle<Result<(), control::Error>>,
    pub reactor: Reactor<wire::Control>,
    pub pool: WorkerPool,
    pub local_addrs: Vec<net::SocketAddr>,
}

impl Runtime {
    /// Run the client.
    ///
    /// This function spawns threads.
    pub fn with<G>(
        home: Home,
        config: service::Config,
        listen: Vec<net::SocketAddr>,
        proxy: net::SocketAddr,
        signer: G,
    ) -> Result<Runtime, Error>
    where
        G: crypto::Signer + EcSign<Sig = Signature, Pk = NodeId> + Clone + 'static,
    {
        let id = *signer.public_key();
        let node_sock = home.socket();
        let node_dir = home.node();
        let network = config.network;
        let rng = fastrand::Rng::new();
        let clock = LocalTime::now();
        let storage = Storage::open(home.storage())?;
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
            signer.clone(),
            rng,
        );

        let cert = Cert {
            pk: id,
            sig: EcSign::sign(&signer, id.as_slice()),
        };

        let (worker_send, worker_recv) = chan::unbounded::<WorkerReq<G>>();
        let mut wire = Wire::new(service, worker_send, cert, signer, proxy, clock);
        let mut local_addrs = Vec::new();

        for addr in listen {
            let listener = NetAccept::bind(&addr)?;
            let local_addr = listener.local_addr();

            local_addrs.push(local_addr);
            wire.listen(listener);

            log::info!("Listening on {local_addr}..");
        }
        let reactor = Reactor::named(wire, popol::Poller::new(), id.to_human())?;
        let handle = Handle::new(home, reactor.controller());
        let control = thread::spawn({
            let handle = handle.clone();
            move || control::listen(node_sock, handle)
        });

        let pool = WorkerPool::with(
            8,
            time::Duration::from_secs(9),
            storage,
            worker_recv,
            handle.clone(),
            id.to_human(),
        );

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
        self.reactor.join().unwrap();
        self.control.join().unwrap()?;

        log::debug!("Node shutdown completed for {}", self.id);

        Ok(())
    }
}
