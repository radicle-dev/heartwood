pub mod handle;
pub mod thread;

use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{fs, io, net};

use crossbeam_channel as chan;
use cyphernet::Ecdh;
use netservices::resource::NetAccept;
use radicle_fetch::FetchLimit;
use reactor::poller::popol;
use reactor::Reactor;
use thiserror::Error;

use radicle::node;
use radicle::node::address;
use radicle::node::address::Store as _;
use radicle::node::notifications;
use radicle::node::Handle as _;
use radicle::profile::Home;
use radicle::storage;
use radicle::Storage;
use radicle::{cob, git};

use crate::control;
use crate::crypto::Signer;
use crate::node::{routing, NodeId};
use crate::service::message::NodeAnnouncement;
use crate::service::{gossip, policy, Event};
use crate::wire;
use crate::wire::{Decode, Wire};
use crate::worker;
use crate::{service, LocalTime};

pub use handle::Error as HandleError;
pub use handle::Handle;

/// A client error.
#[derive(Error, Debug)]
pub enum Error {
    /// A routing database error.
    #[error("routing database error: {0}")]
    Routing(#[from] routing::Error),
    /// A cobs cache database error.
    #[error("cobs cache database error: {0}")]
    CobsCache(#[from] cob::cache::Error),
    /// A node database error.
    #[error("node database error: {0}")]
    Database(#[from] node::db::Error),
    /// A storage error.
    #[error("storage error: {0}")]
    Storage(#[from] storage::Error),
    /// A policies database error.
    #[error("policies database error: {0}")]
    Policy(#[from] policy::Error),
    /// A notifications database error.
    #[error("notifications database error: {0}")]
    Notifications(#[from] notifications::Error),
    /// A gossip database error.
    #[error("gossip database error: {0}")]
    Gossip(#[from] gossip::Error),
    /// An address database error.
    #[error("address database error: {0}")]
    Address(#[from] address::Error),
    /// A service error.
    #[error("service error: {0}")]
    Service(#[from] service::Error),
    /// An I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    /// A control socket error.
    #[error("control socket error: {0}")]
    Control(#[from] control::Error),
    /// Another node is already running.
    #[error(
        "another node appears to be running; \
        if this isn't the case, delete the socket file at '{0}' \
        and restart the node"
    )]
    AlreadyRunning(PathBuf),
    /// A git version error.
    #[error("git version error: {0}")]
    GitVersion(#[from] git::VersionError),
}

/// Publishes events to subscribers.
#[derive(Debug, Clone)]
pub struct Emitter<T> {
    subscribers: Arc<Mutex<Vec<chan::Sender<T>>>>,
}

impl<T> Default for Emitter<T> {
    fn default() -> Emitter<T> {
        Emitter {
            subscribers: Default::default(),
        }
    }
}

impl<T: Clone> Emitter<T> {
    /// Emit event to subscribers and drop those who can't receive it.
    pub(crate) fn emit(&self, event: T) {
        self.subscribers
            .lock()
            .unwrap()
            .retain(|s| s.try_send(event.clone()).is_ok());
    }

    /// Subscribe to events stream.
    pub fn subscribe(&self) -> chan::Receiver<T> {
        let (sender, receiver) = chan::unbounded();
        let mut subs = self.subscribers.lock().unwrap();
        subs.push(sender);

        receiver
    }
}

/// Holds join handles to the client threads, as well as a client handle.
pub struct Runtime {
    pub id: NodeId,
    pub home: Home,
    pub control: UnixListener,
    pub handle: Handle,
    pub storage: Storage,
    pub reactor: Reactor<wire::Control, popol::Poller>,
    pub pool: worker::Pool,
    pub local_addrs: Vec<net::SocketAddr>,
    pub signals: chan::Receiver<()>,
}

impl Runtime {
    /// Initialize the runtime.
    ///
    /// This function spawns threads.
    pub fn init<G: Signer + Ecdh + 'static>(
        home: Home,
        config: service::Config,
        listen: Vec<net::SocketAddr>,
        proxy: net::SocketAddr,
        signals: chan::Receiver<()>,
        signer: G,
    ) -> Result<Runtime, Error>
    where
        G: Ecdh<Pk = NodeId> + Clone,
    {
        let id = *signer.public_key();
        let alias = config.alias.clone();
        let node_dir = home.node();
        let network = config.network;
        let rng = fastrand::Rng::new();
        let clock = LocalTime::now();
        let storage = Storage::open(home.storage(), git::UserInfo { alias, key: id })?;
        let scope = config.scope;
        let policy = config.policy;

        log::info!(target: "node", "Opening node database..");
        let db = home.database_mut()?;
        let mut stores: service::Stores<_> = db.clone().into();

        log::info!(target: "node", "Opening policy database..");
        let policies = home.policies_mut()?;
        let policies = policy::Config::new(policy, scope, policies);
        let notifications = home.notifications_mut()?;
        let cobs_cache = cob::cache::Store::open(home.cobs().join(cob::cache::COBS_DB_FILE))?;

        log::info!(target: "node", "Default seeding policy set to '{}'", &policy);
        log::info!(target: "node", "Initializing service ({:?})..", network);

        let announcement = if let Some(ann) = fs::read(node_dir.join(node::NODE_ANNOUNCEMENT_FILE))
            .ok()
            .and_then(|ann| NodeAnnouncement::decode(&mut ann.as_slice()).ok())
            .and_then(|ann| {
                // If our announcement was made some time ago, the timestamp on it will be old,
                // and it might not get gossiped to new nodes since it will be purged from caches.
                // Therefore, we make sure it's never too old.
                if clock - ann.timestamp.to_local_time() <= config.limits.gossip_max_age {
                    Some(ann)
                } else {
                    None
                }
            })
            .and_then(|ann| {
                if config.features() == ann.features
                    && config.alias == ann.alias
                    && config.external_addresses == ann.addresses.as_ref()
                {
                    Some(ann)
                } else {
                    None
                }
            }) {
            log::info!(
                target: "node",
                "Loaded existing node announcement from file (timestamp={}, work={})",
                ann.timestamp,
                ann.work(),
            );
            ann
        } else {
            service::gossip::node(&config, clock.into())
                .solve(Default::default())
                .expect("Runtime::init: unable to solve proof-of-work puzzle")
        };

        if config.connect.is_empty() && stores.addresses().is_empty()? {
            log::info!(target: "node", "Address book is empty. Adding bootstrap nodes..");

            for (alias, addr) in config.network.bootstrap() {
                let (id, addr) = addr.into();

                stores.addresses_mut().insert(
                    &id,
                    radicle::node::Features::SEED,
                    alias,
                    0,
                    clock.into(),
                    [node::KnownAddress::new(addr, address::Source::Bootstrap)],
                )?;
            }
            log::info!(target: "node", "{} nodes added to address book", stores.addresses().len()?);
        }

        let emitter: Emitter<Event> = Default::default();
        let mut service = service::Service::new(
            config.clone(),
            clock,
            stores,
            storage.clone(),
            policies,
            signer.clone(),
            rng,
            announcement,
            emitter.clone(),
        );
        service.initialize(clock)?;

        let (worker_send, worker_recv) = chan::unbounded::<worker::Task>();
        let mut wire = Wire::new(service, worker_send, signer.clone(), proxy);
        let mut local_addrs = Vec::new();

        for addr in listen {
            let listener = NetAccept::bind(&addr)?;
            let local_addr = listener.local_addr();

            local_addrs.push(local_addr);
            wire.listen(listener);
        }
        let reactor = Reactor::named(wire, popol::Poller::new(), thread::name(&id, "service"))?;
        let handle = Handle::new(home.clone(), reactor.controller(), emitter);

        let nid = *signer.public_key();
        let fetch = worker::FetchConfig {
            limit: FetchLimit::default(),
            local: nid,
            expiry: worker::garbage::Expiry::default(),
        };
        let pool = worker::Pool::with(
            worker_recv,
            nid,
            handle.clone(),
            notifications,
            cobs_cache,
            db,
            worker::Config {
                capacity: config.workers,
                storage: storage.clone(),
                fetch,
                policy,
                scope,
                policies_db: home.node().join(node::POLICIES_DB_FILE),
            },
        )?;
        let control = match UnixListener::bind(home.socket()) {
            Ok(sock) => sock,
            Err(err) if err.kind() == io::ErrorKind::AddrInUse => {
                return Err(Error::AlreadyRunning(home.socket()));
            }
            Err(err) => {
                return Err(err.into());
            }
        };

        Ok(Runtime {
            id,
            home,
            control,
            storage,
            reactor,
            handle,
            pool,
            signals,
            local_addrs,
        })
    }

    pub fn run(self) -> Result<(), Error> {
        let home = self.home;

        log::info!(target: "node", "Running node {} in {}..", self.id, home.path().display());
        log::info!(target: "node", "Binding control socket {}..", home.socket().display());

        thread::spawn(&self.id, "control", {
            let handle = self.handle.clone();
            || control::listen(self.control, handle)
        });
        let _signals = thread::spawn(&self.id, "signals", move || {
            if let Ok(()) = self.signals.recv() {
                log::info!(target: "node", "Termination signal received; shutting down..");
                self.handle.shutdown().ok();
            }
        });

        self.pool.run().unwrap();
        self.reactor.join().unwrap();

        // Nb. We don't join the control thread here, as we have no way of notifying it that the
        // node is shutting down.

        // Remove control socket file, but don't freak out if it's not there anymore.
        fs::remove_file(home.socket()).ok();

        log::debug!(target: "node", "Node shutdown completed for {}", self.id);

        Ok(())
    }
}
