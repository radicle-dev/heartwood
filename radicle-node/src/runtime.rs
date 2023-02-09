mod handle;

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::{fs, io, net, thread, time};

use crossbeam_channel as chan;
use cyphernet::{Cert, EcSign};
use netservices::resource::NetAccept;
use radicle::git;
use radicle::profile::Home;
use radicle::Storage;
use reactor::poller::popol;
use reactor::Reactor;
use thiserror::Error;

use crate::address;
use crate::control;
use crate::crypto::{Signature, Signer};
use crate::node::NodeId;
use crate::service::{routing, tracking};
use crate::wire;
use crate::wire::Wire;
use crate::worker;
use crate::{service, LocalTime};

pub use handle::Error as HandleError;
pub use handle::Handle;

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

/// Holds join handles to the client threads, as well as a client handle.
pub struct Runtime<G: Signer + EcSign> {
    pub id: NodeId,
    pub home: Home,
    pub handle: Handle<G>,
    pub storage: Storage,
    pub reactor: Reactor<wire::Control<G>>,
    pub daemon: net::SocketAddr,
    pub pool: worker::Pool,
    pub local_addrs: Vec<net::SocketAddr>,
}

impl<G: Signer + EcSign + 'static> Runtime<G> {
    /// Initialize the runtime.
    ///
    /// This function spawns threads.
    pub fn init(
        home: Home,
        config: service::Config,
        listen: Vec<net::SocketAddr>,
        proxy: net::SocketAddr,
        daemon: net::SocketAddr,
        signer: G,
    ) -> Result<Runtime<G>, Error>
    where
        G: EcSign<Sig = Signature, Pk = NodeId> + Clone,
    {
        let id = *signer.public_key();
        let node_dir = home.node();
        let network = config.network;
        let rng = fastrand::Rng::new();
        let clock = LocalTime::now();
        let storage = Storage::open(home.storage())?;
        let address_db = node_dir.join(ADDRESS_DB_FILE);
        let routing_db = node_dir.join(ROUTING_DB_FILE);
        let tracking_db = node_dir.join(TRACKING_DB_FILE);

        log::info!(target: "node", "Opening address book {}..", address_db.display());
        let addresses = address::Book::open(address_db)?;

        log::info!(target: "node", "Opening routing table {}..", routing_db.display());
        let routing = routing::Table::open(routing_db)?;

        log::info!(target: "node", "Opening tracking policy table {}..", tracking_db.display());
        let tracking = tracking::Config::open(tracking_db)?;

        log::info!(target: "node", "Initializing service ({:?})..", network);
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

        let (worker_send, worker_recv) = chan::unbounded::<worker::Task<G>>();
        let mut wire = Wire::new(service, worker_send, cert, signer, proxy, clock);
        let mut local_addrs = Vec::new();

        for addr in listen {
            let listener = NetAccept::bind(&addr)?;
            let local_addr = listener.local_addr();

            local_addrs.push(local_addr);
            wire.listen(listener);

            log::info!(target: "node", "Listening on {local_addr}..");
        }
        let reactor = Reactor::named(wire, popol::Poller::new(), id.to_human())?;
        let handle = Handle::new(home.clone(), reactor.controller());
        let atomic = git::version()? >= git::VERSION_REQUIRED;

        if !atomic {
            log::warn!(
                target: "node",
                "Disabling atomic fetches; git version >= {} required", git::VERSION_REQUIRED
            );
        }

        let pool = worker::Pool::with(
            worker_recv,
            handle.clone(),
            worker::Config {
                capacity: 8,
                name: id.to_human(),
                timeout: time::Duration::from_secs(9),
                storage: storage.clone(),
                daemon,
                atomic,
            },
        );

        Ok(Runtime {
            id,
            home,
            storage,
            reactor,
            daemon,
            handle,
            pool,
            local_addrs,
        })
    }

    pub fn run(self) -> Result<(), Error> {
        let home = self.home;

        log::info!(target: "node", "Running node {} in {}..", self.id, home.path().display());
        log::info!(target: "node", "Binding control socket {}..", home.socket().display());

        let listener = match UnixListener::bind(home.socket()) {
            Ok(sock) => sock,
            Err(err) if err.kind() == io::ErrorKind::AddrInUse => {
                return Err(Error::AlreadyRunning(home.socket()));
            }
            Err(err) => {
                return Err(err.into());
            }
        };
        let control = thread::Builder::new()
            .name(self.id.to_human())
            .spawn(move || control::listen(listener, self.handle))?;

        log::info!(target: "node", "Spawning git daemon at {}..", self.storage.path().display());

        let mut daemon = daemon::spawn(self.storage.path(), self.daemon)?;
        thread::Builder::new().name(self.id.to_human()).spawn({
            let stderr = daemon.stderr.take().unwrap();
            || {
                for line in BufReader::new(stderr).lines().flatten() {
                    if line.starts_with("fatal") {
                        log::error!(target: "daemon", "{line}");
                    } else {
                        log::debug!(target: "daemon", "{line}");
                    }
                }
            }
        })?;

        self.pool.run().unwrap();
        self.reactor.join().unwrap();
        control.join().unwrap()?;

        daemon::kill(&daemon).ok(); // Ignore error if daemon has already exited, for whatever reason.
        daemon.wait()?;

        fs::remove_file(home.socket()).ok();

        log::debug!(target: "node", "Node shutdown completed for {}", self.id);

        Ok(())
    }
}

pub mod daemon {
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::{env, io, net};

    /// Kill the daemon process.
    pub fn kill(child: &Child) -> io::Result<()> {
        // SAFETY: We use `libc::kill` because `Child::kill` always sends a `SIGKILL` and that doesn't
        // work for us. We need to send a `SIGTERM` to fully reap the child process. This is because
        // `git-daemon` spawns its own children, and isn't able to reap them if it receives
        // a `SIGKILL`.
        let result = unsafe { libc::kill(child.id() as libc::c_int, libc::SIGTERM) };
        match result {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    /// Spawn the daemon process.
    pub fn spawn(storage: &Path, addr: net::SocketAddr) -> io::Result<Child> {
        let storage = storage.canonicalize()?;
        let listen = format!("--listen={}", addr.ip());
        let port = format!("--port={}", addr.port());
        let child = Command::new("git")
            .env_clear()
            .envs(env::vars().filter(|(k, _)| k == "PATH" || k.starts_with("GIT")))
            .envs(radicle::git::env::GIT_DEFAULT_CONFIG)
            .env("GIT_PROTOCOL", "version=2")
            .current_dir(storage)
            .arg("daemon")
            // Make all git directories available.
            .arg("--export-all")
            .arg("--reuseaddr")
            .arg("--max-connections=32")
            .arg("--informative-errors")
            .arg("--verbose")
            // The git "root". Should be our storage path.
            .arg("--base-path=.")
            // Timeout (in seconds) between the moment the connection is established
            // and the client request is received (typically a rather low value,
            // since that should be basically immediate).
            .arg("--init-timeout=3")
            // Timeout (in seconds) for specific client sub-requests.
            // This includes the time it takes for the server to process the sub-request
            // and the time spent waiting for the next client’s request.
            .arg("--timeout=9")
            .arg("--log-destination=stderr")
            .arg(listen)
            .arg(port)
            .stderr(Stdio::piped())
            .spawn()?;

        Ok(child)
    }
}
