pub mod handle;
pub mod thread;

use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::{fs, io, net};

use crossbeam_channel as chan;
use cyphernet::Ecdh;
use netservices::resource::NetAccept;
use radicle::cob::migrate;
use radicle_fetch::FetchLimit;
use radicle_signals::Signal;
use reactor::poller::popol;
use reactor::Reactor;
use thiserror::Error;

use radicle::node;
use radicle::node::address;
use radicle::node::address::Store as _;
use radicle::node::notifications;
use radicle::node::Handle as _;
use radicle::node::UserAgent;
use radicle::profile::Home;
use radicle::{cob, git, storage, Storage};

use crate::control;
use crate::crypto::Signer;
use crate::node::{routing, NodeId};
use crate::service::message::NodeAnnouncement;
use crate::service::{gossip, policy, Event, INITIAL_SUBSCRIBE_BACKLOG_DELTA};
use crate::wire;
use crate::wire::{Decode, Wire};
use crate::worker;
use crate::{service, LocalTime};

pub use handle::Error as HandleError;
pub use handle::Handle;
pub use node::events::Emitter;

/// Maximum pending worker tasks allowed.
pub const MAX_PENDING_TASKS: usize = 1024;

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

/// Wraps a [`UnixListener`] but tracks its origin.
pub enum ControlSocket {
    /// The listener was created by binding to it.
    Bound(UnixListener, PathBuf),
    /// The listener was received via socket activation.
    Received(UnixListener),
}

/// Holds join handles to the client threads, as well as a client handle.
pub struct Runtime {
    pub id: NodeId,
    pub home: Home,
    pub control: ControlSocket,
    pub handle: Handle,
    pub storage: Storage,
    pub reactor: Reactor<wire::Control, popol::Poller>,
    pub pool: worker::Pool,
    pub local_addrs: Vec<net::SocketAddr>,
    pub signals: chan::Receiver<Signal>,
}

impl Runtime {
    /// Initialize the runtime.
    ///
    /// This function spawns threads.
    pub fn init<G>(
        home: Home,
        config: service::Config,
        listen: Vec<net::SocketAddr>,
        signals: chan::Receiver<Signal>,
        signer: G,
    ) -> Result<Runtime, Error>
    where
        G: Signer + Ecdh<Pk = NodeId> + Clone + 'static,
    {
        let id = *signer.public_key();
        let alias = config.alias.clone();
        let node_dir = home.node();
        let network = config.network;
        let rng = fastrand::Rng::new();
        let clock = LocalTime::now();
        let timestamp = clock.into();
        let storage = Storage::open(home.storage(), git::UserInfo { alias, key: id })?;
        let policy = config.seeding_policy.into();

        for (key, _) in &config.extra {
            log::warn!(target: "node", "Unused or deprecated configuration attribute {:?}", key);
        }

        log::info!(target: "node", "Opening policy database..");
        let policies = home.policies_mut()?;
        let policies = policy::Config::new(policy, policies);
        let notifications = home.notifications_mut()?;
        let mut cobs_cache = cob::cache::Store::open(home.cobs().join(cob::cache::COBS_DB_FILE))?;

        match cobs_cache.check_version() {
            Ok(()) => {}
            Err(cob::cache::Error::OutOfDate) => {
                log::info!(target: "node", "Migrating COBs cache..");
                let version = cobs_cache.migrate(migrate::log)?;
                log::info!(target: "node", "Migration of COBs cache complete (version={version})..");
            }
            Err(e) => return Err(e.into()),
        }

        log::info!(target: "node", "Default seeding policy set to '{}'", &policy);
        log::info!(target: "node", "Initializing service ({:?})..", network);

        let announcement = if let Some(ann) = fs::read(node_dir.join(node::NODE_ANNOUNCEMENT_FILE))
            .ok()
            .and_then(|ann| NodeAnnouncement::decode(&mut ann.as_slice()).ok())
            .and_then(|ann| {
                // If our announcement was made some time ago, the timestamp on it will be old,
                // and it might not get gossiped to new nodes since it will be purged from caches.
                // Therefore, we make sure it's never too old.
                if clock - ann.timestamp.to_local_time() <= INITIAL_SUBSCRIBE_BACKLOG_DELTA {
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
            service::gossip::node(&config, timestamp)
                .solve(Default::default())
                .expect("Runtime::init: unable to solve proof-of-work puzzle")
        };

        log::info!(target: "node", "Opening node database..");
        let db = home
            .database_mut()?
            .journal_mode(node::db::JournalMode::default())?
            .init(
                &id,
                announcement.features,
                &announcement.alias,
                &announcement.agent,
                announcement.timestamp,
                announcement.addresses.iter(),
            )?;
        let mut stores: service::Stores<_> = db.clone().into();

        if config.connect.is_empty() && stores.addresses().is_empty()? {
            log::info!(target: "node", "Address book is empty. Adding bootstrap nodes..");

            for (alias, version, addr) in config.network.bootstrap() {
                let (id, addr) = addr.into();

                stores.addresses_mut().insert(
                    &id,
                    version,
                    radicle::node::Features::SEED,
                    &alias,
                    0,
                    &UserAgent::default(),
                    clock.into(),
                    [node::KnownAddress::new(addr, address::Source::Bootstrap)],
                )?;
            }
            log::info!(target: "node", "{} nodes added to address book", stores.addresses().len()?);
        }

        let emitter: Emitter<Event> = Default::default();
        let mut service = service::Service::new(
            config.clone(),
            stores,
            storage.clone(),
            policies,
            signer.clone(),
            rng,
            announcement,
            emitter.clone(),
        );
        service.initialize(clock)?;

        let (worker_send, worker_recv) = chan::bounded::<worker::Task>(MAX_PENDING_TASKS);
        let mut wire = Wire::new(service, worker_send, signer.clone());
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
                policies_db: home.node().join(node::POLICIES_DB_FILE),
            },
        )?;
        let control = Self::bind(home.socket())?;

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
        let (listener, remove) = match self.control {
            ControlSocket::Bound(listener, path) => (listener, Some(path)),
            ControlSocket::Received(listener) => (listener, None),
        };

        log::info!(target: "node", "Running node {} in {}..", self.id, home.path().display());

        thread::spawn(&self.id, "control", {
            let handle = self.handle.clone();
            || control::listen(listener, handle)
        });
        let _signals = thread::spawn(&self.id, "signals", move || loop {
            match self.signals.recv() {
                Ok(Signal::Terminate | Signal::Interrupt) => {
                    log::info!(target: "node", "Termination signal received; shutting down..");
                    self.handle.shutdown().ok();
                    break;
                }
                Ok(Signal::Hangup) => {
                    log::debug!(target: "node", "Hangup signal (SIGHUP) received; ignoring..");
                }
                Ok(Signal::WindowChanged) => {}
                Err(e) => {
                    log::warn!(target: "node", "Signal notifications channel error: {e}");
                    break;
                }
            }
        });

        self.pool.run().unwrap();
        self.reactor.join().unwrap();

        // Nb. We don't join the control thread here, as we have no way of notifying it that the
        // node is shutting down.

        // Remove control socket file, but don't freak out if it's not there anymore.
        remove.map(|path| fs::remove_file(path).ok());

        log::debug!(target: "node", "Node shutdown completed for {}", self.id);

        Ok(())
    }

    #[cfg(all(feature = "systemd", target_family = "unix"))]
    fn receive_listener() -> Option<UnixListener> {
        use std::os::fd::FromRawFd;
        match radicle_systemd::listen_fd("control") {
            Ok(Some(fd)) => {
                // NOTE: Here, we should make a call to [`fstat(2)`](man:fstat(2))
                // and make sure that the file descriptor we received actually
                // is `AF_UNIX`. However, this requires fiddling with
                // `libc` types or another dependency like `nix`, see
                // <https://github.com/lucab/libsystemd-rs/blob/b43fa5e3b5eca3e6aa16a6c2fad87220dc0ad7a0/src/activation.rs#L192-L196>
                // systemd also implements such a check, see
                // <https://github.com/systemd/systemd/blob/v254/src/libsystemd/sd-daemon/sd-daemon.c#L357-L398>
                Some(unsafe {
                    // SAFETY: We take ownership of this FD from systemd,
                    // which guarantees that it is open.
                    UnixListener::from_raw_fd(fd)
                })
            }
            Ok(None) => None,
            Err(err) => {
                log::trace!(target: "node", "Error receiving file descriptors from systemd: {err}");
                None
            }
        }
    }

    fn bind(path: PathBuf) -> Result<ControlSocket, Error> {
        #[cfg(all(feature = "systemd", target_family = "unix"))]
        {
            if let Some(listener) = Self::receive_listener() {
                log::info!(target: "node", "Received control socket.");
                return Ok(ControlSocket::Received(listener));
            }
        }

        log::info!(target: "node", "Binding control socket {}..", &path.display());
        match UnixListener::bind(&path) {
            Ok(sock) => Ok(ControlSocket::Bound(sock, path)),
            Err(err) if err.kind() == io::ErrorKind::AddrInUse => Err(Error::AlreadyRunning(path)),
            Err(err) => Err(err.into()),
        }
    }
}
