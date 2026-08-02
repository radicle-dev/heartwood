pub mod handle;

use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::str::FromStr as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::{fs, io, net, time};

#[cfg(unix)]
use std::os::unix::net::UnixListener;
#[cfg(windows)]
use uds_windows::UnixListener;

use crate::control;
use crate::worker::TaskResult;
use crate::worker::{self, Worker};
use handle::Handle;
use iroh::endpoint::{
    ConnectError, ConnectWithOptsError, Connection, RecvStream, SendStream, presets,
};
use iroh::protocol::{AcceptError, Router};
use iroh::{Endpoint, RelayMode};
use localtime::LocalTime;
use protocol::service;
use protocol::service::DisconnectReason;
use protocol::service::Metrics;
use protocol::service::session;
use radicle::cob::migrate;
use radicle::crypto::{Signer as _, SigningKey};
use radicle::node::address::Store as _;
use radicle::node::config::FetchPackSizeLimit;
use radicle::node::events::Emitter;
use radicle::node::notifications;
use radicle::node::policy::config as policy;
use radicle::node::{self, Address, Event, Link, UserAgent};
use radicle::node::{NodeId, routing};
use radicle::profile::Home;
use radicle::{Storage, cob, git, storage};
use radicle_protocol::service::io::Io;
use radicle_protocol::worker::{FetchRequest, FetchResult};
use radicle_signals::Signal;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio_util::io::SyncIoBridge;

/// Re-export MPSC from both Tokio and the Standard Library, so we can use them
/// in the same file without having to fully qualify them.
mod mpsc {
    pub(super) use std::sync::mpsc as std;
    pub(super) use tokio::sync::mpsc as tokio;
}

/// How long shutdown waits for network tasks to finish before aborting them.
const NETWORK_TASK_SHUTDOWN_TIMEOUT: time::Duration = time::Duration::from_secs(3);

/// How long shutdown waits for iroh to drain and acknowledge connections.
const IROH_SHUTDOWN_TIMEOUT: time::Duration = time::Duration::from_secs(3);

const ALPN_GOSSIP: &[u8] = b"radicle/gossip/1";
const ALPN_GIT: &[u8] = b"radicle/git/1";

/// A command delivered to the synchronous service actor.
pub(crate) enum ServiceInput {
    User(service::Command),
    Worker(TaskResult),
    Attempted(NodeId, Address),
    Connected(NodeId, Address, Link),
    Disconnected(NodeId, Link, DisconnectReason),
    Message(NodeId, service::Message),
    Listening(net::SocketAddr),
    FetchFailed {
        rid: radicle::identity::RepoId,
        remote: NodeId,
        error: String,
    },
    Shutdown,
}

/// Cloneable synchronous entry point into the service actor.
#[derive(Clone, Debug)]
pub(crate) struct Controller(mpsc::std::Sender<ServiceInput>);

impl Controller {
    pub fn send(
        &self,
        input: ServiceInput,
    ) -> Result<(), mpsc::std::SendError<PhantomData<ServiceInput>>> {
        self.0
            .send(input)
            .map_err(|_: mpsc::std::SendError<ServiceInput>| mpsc::std::SendError(PhantomData))
    }
}

/// Runtime initialization or execution error.
#[derive(Error, Debug)]
pub enum Error {
    #[error("routing database error: {0}")]
    Routing(#[from] routing::Error),
    #[error("cobs cache database error: {0}")]
    CobsCache(#[from] cob::cache::Error),
    #[error("node database error: {0}")]
    Database(#[from] node::db::Error),
    #[error("storage error: {0}")]
    Storage(#[from] storage::Error),
    #[error("policies database error: {0}")]
    Policy(#[from] policy::Error),
    #[error("notifications database error: {0}")]
    Notifications(#[from] notifications::Error),
    #[error("gossip database error: {0}")]
    Gossip(#[from] service::gossip::Error),
    #[error("address database error: {0}")]
    Address(#[from] node::address::Error),
    #[error("service error: {0}")]
    Service(Box<service::Error>),
    #[error("failed to send message to service")]
    ServiceSend,
    #[error("control socket error: {0}")]
    Control(#[from] control::Error),
    #[error("iroh error: {0}")]
    Iroh(String),
    #[error("unsupported legacy transport configuration: {0}")]
    UnsupportedTransport(&'static str),
    #[error(
        "another node appears to be running; if this isn't the case, delete the socket file at '{0}' and restart the node"
    )]
    AlreadyRunning(PathBuf),
    #[error("git version error: {0}")]
    GitVersion(#[from] git::VersionError),
    #[error("failed to spawn thread: {0}")]
    Spawn(std::io::Error),
}

impl From<service::Error> for Error {
    fn from(e: service::Error) -> Self {
        Self::Service(Box::new(e))
    }
}

impl From<mpsc::std::SendError<PhantomData<ServiceInput>>> for Error {
    fn from(_: mpsc::std::SendError<PhantomData<ServiceInput>>) -> Self {
        Self::ServiceSend
    }
}

/// A control listener together with its ownership provenance.
pub enum ControlSocket {
    Bound(UnixListener, PathBuf),
    Received(UnixListener),
}

enum RuntimeOutput {
    Io(Io),
    Shutdown,
}

/// Node runtime. All database-owning state is moved to the service thread.
pub struct Runtime {
    control: ControlSocket,
    pub(crate) handle: Handle,
    signals: mpsc::std::Receiver<Signal>,
    config: node::Config,
    listen: Vec<net::SocketAddr>,
    secret_key: SigningKey,
    controller: Controller,
    output: mpsc::tokio::UnboundedReceiver<RuntimeOutput>,
    service_thread: std::thread::JoinHandle<()>,
    emitter: Emitter<Event>,

    notifications: notifications::StoreWriter,
    cache: cob::cache::StoreWriter,
    db: radicle::node::Database,
    worker_config: crate::worker::Config,
}

impl Runtime {
    /// Initialize databases, blocking actors, channels, workers and control socket.
    pub fn init(
        home: Home,
        config: node::Config,
        socket: PathBuf,
        listen: Vec<net::SocketAddr>,
        signals: mpsc::std::Receiver<Signal>,
        secret_key: SigningKey,
    ) -> Result<Runtime, Error> {
        if config.proxy.is_some() {
            return Err(Error::UnsupportedTransport("SOCKS5 proxy"));
        }
        #[cfg(feature = "tor")]
        if config.onion != node::config::AddressConfig::Drop {
            return Err(Error::UnsupportedTransport("Tor"));
        }
        #[cfg(feature = "i2p")]
        if config.i2p != node::config::AddressConfig::Drop {
            return Err(Error::UnsupportedTransport("I2P"));
        }

        let id = NodeId::from(*secret_key.public_key());
        let alias = config.alias.clone();
        let network = config.network;
        let rng = fastrand::Rng::new();
        let clock = LocalTime::now();
        let storage = Storage::open(home.storage(), git::UserInfo { alias, key: id })?;
        let seeding_policy = config.seeding_policy.into();

        for (key, _) in &config.extra {
            log::warn!(target: "node", "Unused or deprecated configuration attribute {key:?}");
        }

        let policies = policy::Config::new(seeding_policy, home.policies_mut()?);
        let notifications = home.notifications_mut()?;
        let mut cobs_cache = cob::cache::Store::open(home.cobs().join(cob::cache::COBS_DB_FILE))?;
        if let Err(cob::cache::Error::OutOfDate) = cobs_cache.check_version() {
            cobs_cache.migrate(migrate::log)?;
        } else {
            cobs_cache.check_version()?;
        }

        log::info!(target: "node", "Initializing service ({network:?})..");

        let announcement = service::gossip::node(&config, clock.into())
            .solve(Default::default())
            .expect("unable to solve node announcement proof of work");
        let db = home.database_mut(config.database)?.init(
            &id,
            announcement.features,
            &announcement.alias,
            &announcement.agent,
            announcement.timestamp,
            announcement.addresses.iter(),
        )?;
        let mut stores: service::Stores<_> = db.clone().into();
        if config.connect.is_empty() && stores.addresses().is_empty()? {
            for (alias, version, addrs) in network.bootstrap() {
                for addr in addrs {
                    let (nid, addr) = addr.into_pair();
                    stores.addresses_mut().insert(
                        &nid,
                        version,
                        node::Features::SEED,
                        &alias,
                        0,
                        &UserAgent::from_str("/radicle/runtime/bootstrap/")
                            .expect("valid user agent"),
                        clock.into(),
                        [node::KnownAddress::new(
                            addr,
                            node::address::Source::Bootstrap,
                        )],
                    )?;
                }
            }
        }

        let emitter: Emitter<Event> = Default::default();
        let mut service = service::Service::new(
            config.clone(),
            stores,
            storage.clone(),
            policies,
            secret_key.clone(),
            rng,
            announcement,
            emitter.clone(),
        );
        service.initialize(clock)?;

        let (input_tx, input_rx) = mpsc::std::channel();
        let controller = Controller(input_tx);

        let (output_tx, output) = mpsc::tokio::unbounded_channel();

        let service_thread = std::thread::Builder::new()
            .name("service".to_owned())
            .spawn(move || run_service(service, input_rx, output_tx))
            .map_err(Error::Spawn)?;

        let handle = Handle::new(socket.clone(), controller.clone(), emitter.clone());
        let fetch = worker::FetchConfig {
            local: id,
            expiry: worker::garbage::Expiry::default(),
        };
        let control = Self::bind(socket)?;

        let capacity = config.workers.into();

        Ok(Self {
            control,
            handle,
            signals,
            config,
            listen,
            secret_key,
            controller,
            output,
            service_thread,
            emitter,
            notifications,
            cache: cobs_cache,
            db,
            worker_config: worker::Config::new(
                capacity,
                storage,
                fetch,
                seeding_policy,
                home.node().join(node::POLICIES_DB_FILE),
            ),
        })
    }

    /// Bind iroh and run both ALPN protocols until shutdown.
    pub async fn run(mut self) -> Result<(), Error> {
        let (listener, remove) = match self.control {
            ControlSocket::Bound(listener, path) => (listener, Some(path)),
            ControlSocket::Received(listener) => (listener, None),
        };

        let address_discovery = crate::iroh::address_discovery::AddressDiscovery::new(
            &self.db,
            self.config.network,
            &self.emitter,
        )
        .await?;

        let mut builder = if self.config.network == node::config::Network::Test {
            Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled)
        } else {
            Endpoint::builder(crate::iroh::presets::Radicle)
        };

        builder = builder.address_lookup(address_discovery.lookup());
        builder = builder.secret_key(iroh::SecretKey::from(self.secret_key.as_bytes()));
        if !self.listen.is_empty() {
            builder = builder.clear_ip_transports();
            for addr in &self.listen {
                builder = builder
                    .bind_addr(*addr)
                    .map_err(|e| Error::Iroh(e.to_string()))?;
            }
            for address in &self.config.external_addresses {
                match address {
                    Address::Ipv4 { host, port } => {
                        builder = builder.external_addr(net::SocketAddr::V4(
                            net::SocketAddrV4::new(*host, *port),
                        ));
                    }
                    Address::Ipv6 { host, port, .. } => {
                        builder = builder.external_addr(net::SocketAddr::V6(
                            net::SocketAddrV6::new(*host, *port, 0, 0),
                        ));
                    }
                    Address::Dns { host, port } => {
                        match tokio::net::lookup_host((host.as_str(), *port)).await {
                            Ok(addrs) => {
                                for addr in addrs {
                                    builder = builder.external_addr(addr);
                                }
                            }
                            Err(err) => {
                                log::debug!(target: "node", "Unable to resolve external address {host}:{port}: {err}");
                            }
                        }
                    }
                    #[cfg(feature = "tor")]
                    address @ Address::Tor { .. } => {
                        log::debug!(target: "node", "External address '{address}' ignored.");
                    }
                    #[cfg(feature = "i2p")]
                    address @ Address::I2p { .. } => {
                        log::debug!(target: "node", "External address '{address}' ignored.");
                    }
                    Address::Iroh => {
                        // This is implicit, no need to set an external address.
                    }
                    address => {
                        log::warn!(target: "node", "Unsupported external address '{address}' ignored.");
                    }
                }
            }
        } else if !self.config.external_addresses.is_empty() {
            log::debug!(target: "node", "Configured to listen, but no external addresses were provided.");
        }

        let endpoint = builder
            .bind()
            .await
            .map_err(|e| Error::Iroh(e.to_string()))?;
        for addr in endpoint.bound_sockets() {
            self.controller.send(ServiceInput::Listening(addr))?;
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let gossip = Arc::new(SharedForGossip {
            local: *self.secret_key.public_key(),
            endpoint: endpoint.clone(),
            controller: self.controller.clone(),
            peers: tokio::sync::Mutex::new(HashMap::new()),
            jobs: AtomicU64::new(1),
            shutdown: shutdown_rx,
        });

        let worker = Arc::new(SharedForWorker {
            controller: self.controller.clone(),
            jobs: AtomicU64::new(1),
            emitter: self.emitter.clone(),
            cache: self.cache.clone(),
            db: self.db.clone(),
            config: self.worker_config,
            notifications: self.notifications,
        });

        let router = Router::builder(endpoint.clone())
            .accept(
                ALPN_GOSSIP,
                GossipProtocolHandler {
                    shared: gossip.clone(),
                },
            )
            .accept(
                ALPN_GIT,
                GitProtocolHandler {
                    shared: worker.clone(),
                },
            )
            .spawn();

        let stopping = Arc::new(AtomicBool::new(false));
        listener
            .set_nonblocking(true)
            .map_err(|source| Error::Control(control::Error::Bind(source)))?;
        let control_thread = std::thread::Builder::new()
            .name("control-listener".to_owned())
            .spawn({
                let stop = stopping.clone();
                let handle = self.handle.clone();
                move || control::listen(listener, handle, stop)
            })
            .map_err(Error::Spawn)?;

        let mut poll = tokio::time::interval(time::Duration::from_millis(100));
        let mut tasks = JoinSet::new();
        let result = loop {
            tokio::select! {
                Some(output) = self.output.recv() => match output {
                    RuntimeOutput::Io(io) => dispatch(io, gossip.clone(), worker.clone(), endpoint.clone(), &mut tasks).await,
                    RuntimeOutput::Shutdown => break Ok(()),
                },
                Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                    if let Err(err) = result {
                        log::warn!(target: "node", "Network task failed: {err}");
                    }
                }
                _ = poll.tick() => {
                    address_discovery.update().await;
                    while let Ok(signal) = self.signals.try_recv() {
                        match signal {
                            Signal::Terminate | Signal::Interrupt => {
                                let _ = self.controller.send(ServiceInput::Shutdown);
                            }
                            Signal::Hangup | Signal::WindowChanged => {}
                        }
                    }
                }
            }
        };

        stopping.store(true, Ordering::Release);
        let _ = shutdown_tx.send(true);
        if let Some(path) = &remove {
            // Wake a non-blocking control accept loop before joining it.
            let _ = std::os::unix::net::UnixStream::connect(path);
        }
        if tokio::time::timeout(NETWORK_TASK_SHUTDOWN_TIMEOUT, async {
            while tasks.join_next().await.is_some() {}
        })
        .await
        .is_err()
        {
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
        }
        let iroh_error = match tokio::time::timeout(IROH_SHUTDOWN_TIMEOUT, router.shutdown()).await
        {
            Ok(Ok(())) => None,
            Ok(Err(err)) => Some(Error::Iroh(err.to_string())),
            Err(_) => {
                log::warn!(target: "node", "Timed out waiting for iroh endpoint shutdown");
                None
            }
        };
        drop(gossip);
        let _ = control_thread.join();
        let _ = self.service_thread.join();
        if let Some(path) = remove {
            let _ = fs::remove_file(path);
        }
        match iroh_error {
            Some(error) => Err(error),
            None => result,
        }
    }

    #[cfg(all(feature = "socket2", feature = "systemd", target_os = "linux"))]
    fn receive_listener() -> Option<UnixListener> {
        let fd = unsafe { radicle_systemd::listen::fd("control") }.ok()??;
        let socket: socket2::Socket = unsafe { std::os::fd::FromRawFd::from_raw_fd(fd) };
        match socket.domain() {
            Ok(socket2::Domain::UNIX) => Some(UnixListener::from(socket)),
            _ => None,
        }
    }

    fn bind(path: PathBuf) -> Result<ControlSocket, Error> {
        #[cfg(all(feature = "socket2", feature = "systemd", target_os = "linux"))]
        if let Some(listener) = Self::receive_listener() {
            return Ok(ControlSocket::Received(listener));
        }

        log::info!(target: "node", "Binding control socket {}..", path.display());

        match UnixListener::bind(&path) {
            Ok(sock) => Ok(ControlSocket::Bound(sock, path)),
            Err(err) if err.kind() == io::ErrorKind::AddrInUse => Err(Error::AlreadyRunning(path)),
            Err(source) => Err(Error::Control(control::Error::Bind(source))),
        }
    }
}

fn run_service<D, S>(
    mut service: service::Service<D, S>,
    input: mpsc::std::Receiver<ServiceInput>,
    output: mpsc::tokio::UnboundedSender<RuntimeOutput>,
) where
    D: service::Store + Send + 'static,
    S: storage::WriteStorage + Send + 'static,
{
    loop {
        match input.recv_timeout(time::Duration::from_millis(250)) {
            Ok(ServiceInput::User(cmd)) => service.command(cmd),
            Ok(ServiceInput::Worker(task)) => match task.result {
                FetchResult::Initiator { rid, result } => service.fetched(rid, task.remote, result),
                FetchResult::Responder { rid, result } => {
                    if let Some(rid) = rid {
                        log::debug!(target: "worker", "Git upload of {rid} to {} completed: {result:?}", task.remote);
                    }
                }
            },
            Ok(ServiceInput::Attempted(nid, addr)) => service.attempted(nid, addr),
            Ok(ServiceInput::Connected(nid, addr, link)) => service.connected(nid, addr, link),
            Ok(ServiceInput::Disconnected(nid, link, reason)) => {
                service.disconnected(nid, link, &reason)
            }
            Ok(ServiceInput::Message(nid, message)) => service.received_message(nid, message),
            Ok(ServiceInput::Listening(addr)) => service.listening(addr),
            Ok(ServiceInput::FetchFailed { rid, remote, error }) => service.fetched(
                rid,
                remote,
                Err(radicle_protocol::worker::FetchError::Io(io::Error::other(
                    error,
                ))),
            ),
            Ok(ServiceInput::Shutdown) | Err(mpsc::std::RecvTimeoutError::Disconnected) => {
                let _ = output.send(RuntimeOutput::Shutdown);
                break;
            }
            Err(mpsc::std::RecvTimeoutError::Timeout) => {
                service.tick(LocalTime::now(), &Metrics::default());
                service.wake();
            }
        }
        while let Some(io) = service.next() {
            if output.send(RuntimeOutput::Io(io)).is_err() {
                return;
            }
        }
    }
}

struct GossipPeer {
    job: u64,
    link: Link,
    connection: Connection,
    sender: mpsc::tokio::Sender<Vec<service::Message>>,
}

struct SharedForWorker {
    notifications: notifications::StoreWriter,
    cache: cob::cache::StoreWriter,
    db: radicle::node::Database,
    config: crate::worker::Config,

    controller: Controller,
    jobs: AtomicU64,
    emitter: Emitter<Event>,
}

struct SharedForGossip {
    local: NodeId,
    endpoint: Endpoint,
    controller: Controller,
    peers: tokio::sync::Mutex<HashMap<NodeId, GossipPeer>>,
    jobs: AtomicU64,
    shutdown: watch::Receiver<bool>,
}

async fn dispatch(
    io: Io,
    gossip: Arc<SharedForGossip>,
    worker: Arc<SharedForWorker>,
    endpoint: Endpoint,
    tasks: &mut JoinSet<()>,
) {
    match io {
        Io::Write(remote, messages) => {
            let sender = gossip
                .peers
                .lock()
                .await
                .get(&remote)
                .map(|p| p.sender.clone());
            if let Some(sender) = sender {
                let _ = sender.send(messages).await;
            }
        }
        Io::Connect(remote, addr) => {
            let shared = gossip.clone();
            tasks.spawn(async move {
                let _ = shared
                    .controller
                    .send(ServiceInput::Attempted(remote, addr.clone()));

                let id = match iroh::PublicKey::from_bytes(std::borrow::Borrow::borrow(&remote)) {
                    Ok(id) => id,
                    Err(err) => {
                        let _ = shared.controller.send(ServiceInput::Disconnected(
                            remote,
                            Link::Outbound,
                            DisconnectReason::Dial(Arc::new(err)),
                        ));
                        return;
                    }
                };

                match shared.endpoint.connect(id, ALPN_GOSSIP).await {
                    Ok(connection) => {
                        GossipProtocolHandler { shared }
                            .run(connection, Link::Outbound)
                            .await
                    }
                    Err(ConnectError::Connect {
                        source: ConnectWithOptsError::SelfConnect { .. },
                        ..
                    }) => {
                        let _ = shared.controller.send(ServiceInput::Disconnected(
                            remote,
                            Link::Outbound,
                            DisconnectReason::SelfConnection,
                        ));
                    }
                    Err(err @ ConnectError::Connection { .. }) => {
                        let _ = shared.controller.send(ServiceInput::Disconnected(
                            remote,
                            Link::Outbound,
                            DisconnectReason::Connection(Arc::new(err)),
                        ));
                    }
                    Err(err) => {
                        let _ = shared.controller.send(ServiceInput::Disconnected(
                            remote,
                            Link::Outbound,
                            DisconnectReason::Dial(Arc::new(err)),
                        ));
                    }
                }
            });
        }
        Io::Disconnect(remote, reason) => {
            if let Some(peer) = gossip.peers.lock().await.remove(&remote) {
                peer.connection.close(0u32.into(), b"gossip disconnected");
                let _ = gossip
                    .controller
                    .send(ServiceInput::Disconnected(remote, peer.link, reason));
            }
        }
        Io::Fetch {
            rid,
            remote,
            addresses: _,
            refs_at,
            reader_limit,
            config,
        } => {
            tasks.spawn(async move {
                if let Err(error) = run_outgoing_git(
                    worker.clone(),
                    rid,
                    remote,
                    refs_at,
                    reader_limit,
                    config,
                    endpoint,
                )
                .await
                {
                    let _ =
                        gossip
                            .controller
                            .send(ServiceInput::FetchFailed { rid, remote, error });
                }
            });
        }
        Io::Wakeup(_) => {
            // TODO: Handle wakeup.
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_outgoing_git(
    shared: Arc<SharedForWorker>,
    rid: radicle::identity::RepoId,
    remote: NodeId,
    refs_at: Option<Vec<radicle::storage::refs::RefsAt>>,
    reader_limit: node::config::FetchPackSizeLimit,
    config: radicle_protocol::fetcher::FetchConfig,
    endpoint: Endpoint,
) -> Result<(), String> {
    let id = match iroh::PublicKey::from_bytes(std::borrow::Borrow::borrow(&remote)) {
        Ok(id) => id,
        Err(err) => {
            let _ = shared.controller.send(ServiceInput::Disconnected(
                remote,
                Link::Outbound,
                DisconnectReason::Dial(Arc::new(err)),
            ));
            return Err("invalid remote node ID".to_owned());
        }
    };

    let connection = tokio::time::timeout(config.timeout(), endpoint.connect(id, ALPN_GIT))
        .await
        .map_err(|_| "Git dial timed out".to_owned())?
        .map_err(|e| e.to_string())?;
    let (send, recv) = connection.open_bi().await.map_err(|e| e.to_string())?;

    let fetch = FetchRequest::Initiator {
        rid,
        remote,
        refs_at,
        config: config.fetch_config(),
    };
    run_git_worker(shared, send, recv, fetch, config.timeout(), reader_limit).await
}

async fn run_git_worker(
    shared: Arc<SharedForWorker>,
    send: SendStream,
    recv: RecvStream,
    fetch: FetchRequest,
    timeout: time::Duration,
    reader_limit: FetchPackSizeLimit,
) -> Result<(), String> {
    let job = shared.jobs.fetch_add(1, Ordering::Relaxed);

    let limit = reader_limit.as_u64();

    let writer = SyncIoBridge::new(send);
    let reader = SyncIoBridge::new(recv.take(limit));

    let remote = fetch.remote();

    let mut worker = Worker::new(
        shared.config.storage.clone(),
        shared.config.fetch.clone(),
        shared.notifications.clone(),
        shared.cache.clone(),
        shared.db.clone(),
        shared.config.policy,
        shared.config.policies_db.clone(),
        timeout,
    );

    let result = tokio::task::spawn_blocking(move || worker.process(fetch, job, reader, writer))
        .await
        .map_err(|e| e.to_string())?;

    let result = TaskResult {
        remote,
        result,
        job,
    };

    shared
        .controller
        .send(ServiceInput::Worker(result))
        .map_err(|e| e.to_string())?;

    Ok(())
}

struct GossipProtocolHandler {
    shared: Arc<SharedForGossip>,
}

impl GossipProtocolHandler {
    async fn run(&self, connection: Connection, link: Link) {
        let remote = NodeId::from_bytes(*connection.remote_id());

        // For gossip, we only allow one bidirectional stream per connection,
        // as multiple streams do not meaningfully map to the gossip protocol.
        connection.set_max_concurrent_bi_streams(1u8.into());

        // Gossip streams are always bidirectional, so we do not allow
        // any unidirectional stream.
        connection.set_max_concurrent_uni_streams(0u8.into());

        if remote == self.shared.local {
            connection.close(0u32.into(), b"self connection");
            return;
        }

        let preferred = if self.shared.local > remote {
            Link::Outbound
        } else {
            Link::Inbound
        };

        let streams = match link {
            Link::Outbound => connection.open_bi().await,
            Link::Inbound => connection.accept_bi().await,
        };

        let (mut send, mut recv) = match streams {
            Ok(streams) => streams,
            Err(err) => {
                log::debug!(target: "gossip", "Failed to open gossip stream with {remote}: {err}");
                return;
            }
        };

        let job = self.shared.jobs.fetch_add(1, Ordering::Relaxed);

        let (tx, mut rx) = mpsc::tokio::channel(32);
        {
            let mut peers = self.shared.peers.lock().await;
            if let Some(existing) = peers.get(&remote) {
                let new_wins = link == preferred && existing.link != preferred;
                if !new_wins {
                    connection.close(0u32.into(), b"duplicate gossip connection");
                    return;
                }
                existing
                    .connection
                    .close(0u32.into(), b"superseded gossip connection");
            }
            peers.insert(
                remote,
                GossipPeer {
                    job,
                    link,
                    connection: connection.clone(),
                    sender: tx,
                },
            );
        }
        let addr = Address::Iroh;

        let _ = self
            .shared
            .controller
            .send(ServiceInput::Connected(remote, addr, link));

        let mut reason = DisconnectReason::connection();
        let mut shutdown = self.shared.shutdown.clone();

        let receive = async {
            loop {
                match crate::wire::read_message(&mut recv).await {
                    Ok((message, _bytes)) => {
                        match self
                            .shared
                            .controller
                            .send(ServiceInput::Message(remote, message))
                        {
                            Ok(_) => {}
                            Err(err) => {
                                log::warn!(target: "gossip", "Unable to send gossip message from {remote} to service: {err}");
                                return None;
                            }
                        }
                    }
                    Err(err) => return Some(err),
                }
            }
        };

        let transmit = async {
            while let Some(messages) = rx.recv().await {
                for message in messages {
                    crate::wire::write_message(&mut send, &message).await?;
                }
            }
            Ok::<_, crate::wire::Error>(())
        };

        tokio::pin!(receive);
        tokio::pin!(transmit);

        tokio::select! {
            _ = shutdown.changed() => {
                connection.close(0u32.into(), b"node shutting down");
            }
            result = &mut receive => {
                if let Some(err) = result
                    && connection.close_reason().is_none()
                {
                    log::debug!(target: "gossip", "Invalid gossip from {remote}: {err}");
                    reason = DisconnectReason::Session(session::Error::Misbehavior);
                    connection.close(0u32.into(), b"invalid gossip");
                }
            }
            result = &mut transmit => {
                if let Err(err) = result {
                    log::warn!(target: "gossip", "Unable to send gossip to {remote}: {err}");
                    connection.close(0u32.into(), b"unable to encode gossip");
                }
            }
            _ = connection.closed() => {}
        }

        let removed = {
            let mut peers = self.shared.peers.lock().await;
            if peers.get(&remote).is_some_and(|peer| peer.job == job) {
                peers.remove(&remote);
                true
            } else {
                false
            }
        };

        if removed {
            let _ = self
                .shared
                .controller
                .send(ServiceInput::Disconnected(remote, link, reason));
        }
    }
}

impl std::fmt::Debug for GossipProtocolHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GossipProtocolHandler").finish()
    }
}

impl iroh::protocol::ProtocolHandler for GossipProtocolHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        self.run(connection, Link::Inbound).await;
        Ok(())
    }
}

struct GitProtocolHandler {
    shared: Arc<SharedForWorker>,
}

impl GitProtocolHandler {
    async fn run(&self, connection: Connection) {
        let remote = NodeId::from_bytes(*connection.remote_id());
        let Ok((send, recv)) = connection.accept_bi().await else {
            return;
        };

        let fetch = FetchRequest::Responder {
            remote,
            emitter: self.shared.emitter.clone(),
        };
        if let Err(error) = run_git_worker(
            self.shared.clone(),
            send,
            recv,
            fetch,
            service::FETCH_TIMEOUT,
            FetchPackSizeLimit::default(),
        )
        .await
        {
            log::debug!(target: "worker", "Incoming Git connection from {remote} failed: {error}");
        }
    }
}

impl std::fmt::Debug for GitProtocolHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitProtocolHandler").finish()
    }
}

impl iroh::protocol::ProtocolHandler for GitProtocolHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        self.run(connection).await;
        Ok(())
    }
}
