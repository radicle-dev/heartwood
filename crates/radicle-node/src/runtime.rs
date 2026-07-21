pub mod handle;
pub mod thread;

use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::str::FromStr as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::{fs, io, net, time};

#[cfg(unix)]
use std::os::unix::net::UnixListener;
#[cfg(windows)]
use uds_windows::UnixListener;

use crate::control;
use crate::wire::{self, GIT_ALPN, GOSSIP_ALPN};
use crate::worker;
use crate::worker::{ChannelEvent, ChannelsConfig, Task, TaskResult};
use crossbeam_channel as chan;
use handle::Handle;
use iroh::endpoint::{Connection, RecvStream, SendStream, presets};
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, RelayMode};
use localtime::LocalTime;
use protocol::service;
use protocol::service::DisconnectReason;
use protocol::service::Metrics;
use protocol::service::session;
use radicle::cob::migrate;
use radicle::crypto::{Signer as _, SigningKey};
use radicle::node::address::Store as _;
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
use tokio::sync::{Semaphore, mpsc, oneshot, watch};
use tokio::task::JoinSet;

/// Maximum pending worker tasks allowed.
pub const MAX_PENDING_TASKS: usize = 1024;

/// How long a Git supervisor waits for its blocking worker after closing the
/// byte-stream bridge.
const GIT_WORKER_DRAIN_TIMEOUT: time::Duration = time::Duration::from_secs(2);

/// How long shutdown waits for network tasks to finish before aborting them.
const NETWORK_TASK_SHUTDOWN_TIMEOUT: time::Duration = time::Duration::from_secs(3);

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
pub(crate) struct Controller(chan::Sender<ServiceInput>);

impl Controller {
    pub fn send(
        &self,
        input: ServiceInput,
    ) -> Result<(), chan::SendError<PhantomData<ServiceInput>>> {
        self.0
            .send(input)
            .map_err(|_: chan::SendError<ServiceInput>| chan::SendError(PhantomData))
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
}

impl From<service::Error> for Error {
    fn from(e: service::Error) -> Self {
        Self::Service(Box::new(e))
    }
}

impl From<chan::SendError<PhantomData<ServiceInput>>> for Error {
    fn from(_: chan::SendError<PhantomData<ServiceInput>>) -> Self {
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
    pool: worker::Pool,
    signals: std::sync::mpsc::Receiver<Signal>,
    config: node::Config,
    listen: Vec<net::SocketAddr>,
    secret_key: SigningKey,
    controller: Controller,
    worker: chan::Sender<Task>,
    output: mpsc::UnboundedReceiver<RuntimeOutput>,
    service_thread: std::thread::JoinHandle<()>,
    emitter: Emitter<Event>,
}

impl Runtime {
    /// Initialize databases, blocking actors, channels, workers and control socket.
    pub fn init(
        home: Home,
        config: node::Config,
        socket: PathBuf,
        listen: Vec<net::SocketAddr>,
        signals: std::sync::mpsc::Receiver<Signal>,
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

        let (input_tx, input_rx) = chan::unbounded();
        let controller = Controller(input_tx);
        let (output_tx, output) = mpsc::unbounded_channel();
        let service_thread = thread::spawn(&id, "service", move || {
            run_service(service, input_rx, output_tx)
        });

        let handle = Handle::new(socket.clone(), controller.clone(), emitter.clone());
        let (worker, worker_recv) = chan::bounded::<Task>(MAX_PENDING_TASKS);
        let fetch = worker::FetchConfig {
            local: id,
            expiry: worker::garbage::Expiry::default(),
        };
        let pool = worker::Pool::with(
            worker_recv,
            id,
            notifications,
            cobs_cache,
            db,
            worker::Config::new(
                config.workers.into(),
                storage.clone(),
                fetch,
                seeding_policy,
                home.node().join(node::POLICIES_DB_FILE),
            ),
        )?;
        let control = Self::bind(socket)?;

        Ok(Self {
            control,
            handle,
            pool,
            signals,
            config,
            listen,
            secret_key,
            controller,
            worker,
            output,
            service_thread,
            emitter,
        })
    }

    /// Bind iroh and run both ALPN protocols until shutdown.
    pub async fn run(mut self) -> Result<(), Error> {
        let (listener, remove) = match self.control {
            ControlSocket::Bound(listener, path) => (listener, Some(path)),
            ControlSocket::Received(listener) => (listener, None),
        };

        let mut builder = if self.config.network == node::config::Network::Test {
            Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled)
        } else {
            Endpoint::builder(presets::N0)
        };

        builder = builder.secret_key(iroh::SecretKey::from(self.secret_key.as_bytes()));
        if !self.listen.is_empty() {
            builder = builder.clear_ip_transports();
            for addr in &self.listen {
                builder = builder
                    .bind_addr(*addr)
                    .map_err(|e| Error::Iroh(e.to_string()))?;
            }
        }
        let endpoint = builder
            .bind()
            .await
            .map_err(|e| Error::Iroh(e.to_string()))?;
        for addr in endpoint.bound_sockets() {
            self.controller.send(ServiceInput::Listening(addr))?;
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let shared = Arc::new(Shared {
            local: *self.secret_key.public_key(),
            endpoint: endpoint.clone(),
            controller: self.controller.clone(),
            worker: self.worker,
            emitter: self.emitter.clone(),
            peers: tokio::sync::Mutex::new(HashMap::new()),
            jobs: AtomicU64::new(1),
            incoming_git_limits: Mutex::new(HashMap::new()),
            outgoing_git_limits: Mutex::new(HashMap::new()),
            git_concurrency: self.config.limits.fetch_concurrency.into(),
            shutdown: shutdown_rx,
        });

        let (incoming_tx, mut incoming_rx) = mpsc::unbounded_channel();
        let router = Router::builder(endpoint)
            .accept(
                GOSSIP_ALPN,
                IncomingHandler {
                    kind: Protocol::Gossip,
                    tx: incoming_tx.clone(),
                },
            )
            .accept(
                GIT_ALPN,
                IncomingHandler {
                    kind: Protocol::Git,
                    tx: incoming_tx,
                },
            )
            .spawn();

        let stopping = Arc::new(AtomicBool::new(false));
        listener
            .set_nonblocking(true)
            .map_err(|source| Error::Control(control::Error::Bind(source)))?;
        let control_thread = thread::spawn(self.secret_key.public_key(), "control-listener", {
            let stop = stopping.clone();
            let handle = self.handle.clone();
            move || control::listen(listener, handle, stop)
        });

        let mut poll = tokio::time::interval(time::Duration::from_millis(100));
        let mut tasks = JoinSet::new();
        let result = loop {
            tokio::select! {
                Some(incoming) = incoming_rx.recv() => {
                    let shared = shared.clone();
                    tasks.spawn(async move {
                        match incoming {
                            Incoming::Gossip(connection) => run_gossip(shared, connection, Link::Inbound).await,
                            Incoming::Git(connection) => run_incoming_git(shared, connection).await,
                        }
                    });
                }
                Some(output) = self.output.recv() => match output {
                    RuntimeOutput::Io(io) => dispatch(io, shared.clone(), &mut tasks).await,
                    RuntimeOutput::Shutdown => break Ok(()),
                },
                Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                    if let Err(err) = result {
                        log::warn!(target: "node", "Network task failed: {err}");
                    }
                }
                _ = poll.tick() => {
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
        router
            .shutdown()
            .await
            .map_err(|e| Error::Iroh(e.to_string()))?;
        if tokio::time::timeout(NETWORK_TASK_SHUTDOWN_TIMEOUT, async {
            while tasks.join_next().await.is_some() {}
        })
        .await
        .is_err()
        {
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
        }
        drop(shared);
        let _ = control_thread.join();
        let _ = self.pool.run();
        let _ = self.service_thread.join();
        if let Some(path) = remove {
            let _ = fs::remove_file(path);
        }
        result
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
    input: chan::Receiver<ServiceInput>,
    output: mpsc::UnboundedSender<RuntimeOutput>,
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
            Ok(ServiceInput::Shutdown) | Err(chan::RecvTimeoutError::Disconnected) => {
                let _ = output.send(RuntimeOutput::Shutdown);
                break;
            }
            Err(chan::RecvTimeoutError::Timeout) => {
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

#[derive(Debug, Clone, Copy)]
enum Protocol {
    Gossip,
    Git,
}

enum Incoming {
    Gossip(Connection),
    Git(Connection),
}

#[derive(Debug, Clone)]
struct IncomingHandler {
    kind: Protocol,
    tx: mpsc::UnboundedSender<Incoming>,
}

impl ProtocolHandler for IncomingHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let incoming = match self.kind {
            Protocol::Gossip => Incoming::Gossip(connection),
            Protocol::Git => Incoming::Git(connection),
        };
        self.tx
            .send(incoming)
            .map_err(|_| AcceptError::from_err(io::Error::from(io::ErrorKind::BrokenPipe)))
    }
}

struct GossipPeer {
    job: u64,
    link: Link,
    connection: Connection,
    sender: mpsc::Sender<Vec<service::Message>>,
}

struct Shared {
    local: NodeId,
    endpoint: Endpoint,
    controller: Controller,
    worker: chan::Sender<Task>,
    emitter: Emitter<Event>,
    peers: tokio::sync::Mutex<HashMap<NodeId, GossipPeer>>,
    jobs: AtomicU64,
    incoming_git_limits: Mutex<HashMap<NodeId, Arc<Semaphore>>>,
    outgoing_git_limits: Mutex<HashMap<NodeId, Arc<Semaphore>>>,
    git_concurrency: usize,
    shutdown: watch::Receiver<bool>,
}

async fn dispatch(io: Io, shared: Arc<Shared>, tasks: &mut JoinSet<()>) {
    match io {
        Io::Write(remote, messages) => {
            let sender = shared
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
            let shared = shared.clone();
            tasks.spawn(async move {
                let _ = shared
                    .controller
                    .send(ServiceInput::Attempted(remote, addr.clone()));
                match endpoint_addr(remote, std::slice::from_ref(&addr)).await {
                    Ok(endpoint_addr) => {
                        match shared.endpoint.connect(endpoint_addr, GOSSIP_ALPN).await {
                            Ok(connection) => run_gossip(shared, connection, Link::Outbound).await,
                            Err(err) => {
                                let reason = DisconnectReason::Dial(Arc::new(io::Error::other(
                                    err.to_string(),
                                )));
                                let _ = shared.controller.send(ServiceInput::Disconnected(
                                    remote,
                                    Link::Outbound,
                                    reason,
                                ));
                            }
                        }
                    }
                    Err(err) => {
                        let reason = DisconnectReason::Dial(Arc::new(err));
                        let _ = shared.controller.send(ServiceInput::Disconnected(
                            remote,
                            Link::Outbound,
                            reason,
                        ));
                    }
                }
            });
        }
        Io::Disconnect(remote, reason) => {
            if let Some(peer) = shared.peers.lock().await.remove(&remote) {
                peer.connection.close(0u32.into(), b"gossip disconnected");
                let _ = shared
                    .controller
                    .send(ServiceInput::Disconnected(remote, peer.link, reason));
            }
        }
        Io::Fetch {
            rid,
            remote,
            addresses,
            refs_at,
            reader_limit,
            config,
        } => {
            tasks.spawn(async move {
                if let Err(error) = run_outgoing_git(
                    shared.clone(),
                    rid,
                    remote,
                    addresses,
                    refs_at,
                    reader_limit,
                    config,
                )
                .await
                {
                    let _ =
                        shared
                            .controller
                            .send(ServiceInput::FetchFailed { rid, remote, error });
                }
            });
        }
        Io::Wakeup(_) => {}
    }
}

async fn run_gossip(shared: Arc<Shared>, connection: Connection, link: Link) {
    let remote = NodeId::from_bytes(*connection.remote_id());

    if remote == shared.local {
        connection.close(0u32.into(), b"self connection");
        return;
    }
    let preferred = if shared.local > remote {
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
    let job = shared.jobs.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = mpsc::channel(32);
    {
        let mut peers = shared.peers.lock().await;
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
    let _ = shared
        .controller
        .send(ServiceInput::Connected(remote, addr, link));

    let mut reason = DisconnectReason::connection();
    'gossip: loop {
        tokio::select! {
            incoming = wire::read_message(&mut recv) => match incoming {
                Ok((message, _bytes)) => {
                    if shared.controller.send(ServiceInput::Message(remote, message)).is_err() { break; }
                }
                Err(err) => {
                    if connection.close_reason().is_some() {
                        break;
                    }
                    log::debug!(target: "gossip", "Invalid gossip from {remote}: {err}");
                    reason = DisconnectReason::Session(session::Error::Misbehavior);
                    connection.close(0u32.into(), b"invalid gossip");
                    break;
                }
            },
            Some(messages) = rx.recv() => {
                for message in messages {
                    if let Err(err) = crate::wire::write_message(&mut send, &message).await {
                        log::warn!(target: "gossip", "Unable to send gossip to {remote}: {err}");
                        connection.close(0u32.into(), b"unable to encode gossip");
                        break 'gossip;
                    }
                }
            }
            _ = connection.closed() => break,
        }
    }
    let removed = {
        let mut peers = shared.peers.lock().await;
        if peers.get(&remote).is_some_and(|peer| peer.job == job) {
            peers.remove(&remote);
            true
        } else {
            false
        }
    };
    if removed {
        let _ = shared
            .controller
            .send(ServiceInput::Disconnected(remote, link, reason));
    }
}

async fn run_incoming_git(shared: Arc<Shared>, connection: Connection) {
    let remote = NodeId::from_bytes(*connection.remote_id());
    let Ok(permit) = git_limiter(&shared.incoming_git_limits, shared.git_concurrency, remote)
        .try_acquire_owned()
    else {
        connection.close(0u32.into(), b"Git concurrency limit exceeded");
        return;
    };
    let Ok((send, recv)) = connection.accept_bi().await else {
        return;
    };
    let config = ChannelsConfig::new(service::FETCH_TIMEOUT);
    let fetch = FetchRequest::Responder {
        remote,
        emitter: shared.emitter.clone(),
    };
    if let Err(error) = run_git_worker(shared, connection, send, recv, fetch, config).await {
        log::debug!(target: "worker", "Incoming Git connection from {remote} failed: {error}");
    }
    drop(permit);
}

async fn run_outgoing_git(
    shared: Arc<Shared>,
    rid: radicle::identity::RepoId,
    remote: NodeId,
    addresses: Vec<Address>,
    refs_at: Option<Vec<radicle::storage::refs::RefsAt>>,
    reader_limit: node::config::FetchPackSizeLimit,
    config: radicle_protocol::fetcher::FetchConfig,
) -> Result<(), String> {
    let _permit = tokio::time::timeout(config.timeout(), git_permit(&shared, remote))
        .await
        .map_err(|_| "Git concurrency wait timed out".to_owned())??;
    let addr = endpoint_addr(remote, &addresses)
        .await
        .map_err(|e| e.to_string())?;

    let connection =
        tokio::time::timeout(config.timeout(), shared.endpoint.connect(addr, GIT_ALPN))
            .await
            .map_err(|_| "Git dial timed out".to_owned())?
            .map_err(|e| e.to_string())?;
    let (send, recv) = connection.open_bi().await.map_err(|e| e.to_string())?;
    let channels = ChannelsConfig::new(config.timeout()).with_reader_limit(reader_limit);
    let fetch = FetchRequest::Initiator {
        rid,
        remote,
        refs_at,
        config: config.fetch_config(),
    };
    run_git_worker(shared, connection, send, recv, fetch, channels).await
}

async fn run_git_worker(
    shared: Arc<Shared>,
    connection: Connection,
    mut send: SendStream,
    mut recv: RecvStream,
    fetch: FetchRequest,
    config: ChannelsConfig,
) -> Result<(), String> {
    enum PumpResult {
        Worker(Box<TaskResult>),
        WorkerDisconnected(String),
        Failed(String),
    }

    let job = shared.jobs.fetch_add(1, Ordering::Relaxed);
    let timeout = config.timeout();
    let (network, worker_channels) =
        worker::channels::Channels::pair(config).map_err(|e| e.to_string())?;
    let (result_tx, mut result_rx) = oneshot::channel();
    shared
        .worker
        .try_send(Task::new(fetch, job, worker_channels, result_tx))
        .map_err(|_| "worker queue is full".to_owned())?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut recv_open = true;
    let mut send_open = true;
    let mut shutdown = shared.shutdown.clone();
    let pump = loop {
        tokio::select! {
            biased;

            _ = shutdown.changed() => {
                break PumpResult::Failed("Git task cancelled".to_owned());
            }
            result = &mut result_rx => {
                match result {
                    Ok(result) => break PumpResult::Worker(Box::new(result)),
                    Err(_) => break PumpResult::Failed("worker exited without a result".to_owned()),
                }
            }
            read = recv.read(&mut buffer), if recv_open => match read {
                Ok(None) => {
                    recv_open = false;
                    let _ = network.send(ChannelEvent::Eof).await;
                }
                Ok(Some(n)) => {
                    if network.send(ChannelEvent::Data(buffer[..n].to_vec())).await.is_err() {
                        break PumpResult::WorkerDisconnected("worker disconnected".to_owned());
                    }
                }
                Err(err) => break PumpResult::Failed(err.to_string()),
            },
            event = network.recv() => match event {
                Ok(ChannelEvent::Data(data)) if send_open => {
                    if let Err(err) = send.write_all(&data).await {
                        break PumpResult::Failed(err.to_string());
                    }
                }
                Ok(ChannelEvent::Data(_)) => break PumpResult::Failed("worker wrote after closing Git stream".to_owned()),
                Ok(ChannelEvent::Eof) if send_open => {
                    send_open = false;
                    let _ = send.finish();
                }
                Ok(ChannelEvent::Eof) => {}
                Ok(ChannelEvent::Close) => break PumpResult::WorkerDisconnected("worker closed Git stream".to_owned()),
                Err(_) => break PumpResult::WorkerDisconnected("worker disconnected".to_owned()),
            }
        }
    };

    // The worker owns both halves of its byte-stream bridge. They are dropped
    // as `_process` returns, immediately before the task result is sent. Thus a
    // disconnected bridge usually means that the result is about to arrive,
    // rather than that the worker failed to report it.
    let pump = match pump {
        PumpResult::WorkerDisconnected(error) => {
            match tokio::time::timeout(GIT_WORKER_DRAIN_TIMEOUT, &mut result_rx).await {
                Ok(Ok(result)) => PumpResult::Worker(Box::new(result)),
                Ok(Err(_)) => PumpResult::Failed(error),
                Err(_) => PumpResult::Failed(error),
            }
        }
        pump => pump,
    };

    match pump {
        PumpResult::Worker(result) => {
            let _ = send.finish();
            let deadline = tokio::time::Instant::now() + timeout;
            while recv_open {
                match tokio::time::timeout_at(deadline, recv.read(&mut buffer)).await {
                    Ok(Ok(None)) => recv_open = false,
                    Ok(Ok(Some(_))) => {}
                    Ok(Err(err)) => return Err(err.to_string()),
                    Err(_) => return Err("Git stream completion timed out".to_owned()),
                }
            }
            match tokio::time::timeout_at(deadline, send.stopped()).await {
                Ok(Ok(None)) => {}
                Ok(Ok(Some(code))) => return Err(format!("remote stopped Git stream: {code}")),
                Ok(Err(err)) => return Err(err.to_string()),
                Err(_) => return Err("Git stream acknowledgement timed out".to_owned()),
            }
            shared
                .controller
                .send(ServiceInput::Worker(*result))
                .map_err(|e| e.to_string())
        }
        PumpResult::Failed(error) => {
            connection.close(0u32.into(), b"Git task stopped");
            drop(send);
            drop(recv);
            drop(network);

            // A blocking worker cannot be cancelled. Keep its result receiver
            // alive while closing the bridge gives it a chance to unwind.
            match tokio::time::timeout(GIT_WORKER_DRAIN_TIMEOUT, result_rx).await {
                Ok(Ok(_)) => {}
                Ok(Err(_)) => {
                    log::debug!(target: "worker", "Git worker for job {job} exited without a result");
                }
                Err(_) => {
                    log::debug!(target: "worker", "Timed out waiting for Git worker job {job} to stop");
                }
            }
            Err(error)
        }
        PumpResult::WorkerDisconnected(_) => unreachable!("handled above"),
    }
}

async fn git_permit(
    shared: &Arc<Shared>,
    remote: NodeId,
) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    if shared.git_concurrency == 0 {
        return Err("Git concurrency is disabled".to_owned());
    }
    git_limiter(&shared.outgoing_git_limits, shared.git_concurrency, remote)
        .acquire_owned()
        .await
        .map_err(|_| "Git limiter closed".to_owned())
}

fn git_limiter(
    limits: &Mutex<HashMap<NodeId, Arc<Semaphore>>>,
    concurrency: usize,
    remote: NodeId,
) -> Arc<Semaphore> {
    limits
        .lock()
        .expect("Git limiter poisoned")
        .entry(remote)
        .or_insert_with(|| Arc::new(Semaphore::new(concurrency)))
        .clone()
}

async fn endpoint_addr(remote: NodeId, addresses: &[Address]) -> io::Result<EndpointAddr> {
    let id = iroh::PublicKey::from_bytes(std::borrow::Borrow::borrow(&remote))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let mut result = EndpointAddr::new(id);
    for address in addresses {
        match address {
            Address::Ipv4 { host, port } => {
                result =
                    result.with_ip_addr(net::SocketAddr::V4(net::SocketAddrV4::new(*host, *port)));
            }
            Address::Ipv6 { host, port, .. } => {
                result = result.with_ip_addr(net::SocketAddr::V6(net::SocketAddrV6::new(
                    *host, *port, 0, 0,
                )));
            }
            Address::Dns { host, port } => {
                for addr in tokio::net::lookup_host((host.as_str(), *port)).await? {
                    result = result.with_ip_addr(addr);
                }
            }
            #[cfg(feature = "tor")]
            address @ Address::Tor { .. } => {
                // TODO: In the future, create a custom transport address
                // (via `iroh::TransportAddr::Custom`) that can be used to dial
                // Tor addresses.
                log::warn!(target: "node", "Tor transport is not yet supported. Ignoring address '{address}'.");
            }
            #[cfg(feature = "i2p")]
            address @ Address::I2p { .. } => {
                // TODO: In the future, create a custom transport address
                // (via `iroh::TransportAddr::Custom`) that can be used to dial
                // I2P addresses.
                log::warn!(target: "node", "I2P transport is not yet supported. Ignoring address '{address}'.");
            }
            Address::Iroh => {}
            address => {
                log::warn!(target: "node", "Ignoring unsupported address '{address}'.");
            }
        }
    }
    Ok(result)
}
