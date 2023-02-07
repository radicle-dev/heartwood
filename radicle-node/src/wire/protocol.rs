//! Implementation of the transport protocol.
//!
//! We use the Noise NN handshake pattern to establish an encrypted stream with a remote peer.
//! The handshake itself is implemented in the external [`cyphernet`] and [`netservices`] crates.
use std::collections::VecDeque;
use std::os::unix::io::AsRawFd;
use std::os::unix::prelude::RawFd;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use std::{fmt, io, net};

use amplify::Wrapper as _;
use crossbeam_channel as chan;
use cyphernet::{Cert, Digest, EcSign, Sha256};
use localtime::LocalTime;
use netservices::resource::{ListenerEvent, NetAccept, NetTransport, SessionEvent};
use netservices::session::ProtocolArtifact;
use netservices::session::{CypherReader, CypherSession, CypherWriter};
use netservices::{NetConnection, NetSession};

use radicle::collections::HashMap;
use radicle::crypto::Signature;
use radicle::node::NodeId;
use radicle::storage::WriteStorage;

use crate::crypto::Signer;
use crate::service::reactor::{Fetch, Io};
use crate::service::{routing, session, DisconnectReason, Message, Service};
use crate::wire;
use crate::wire::{Decode, Encode};
use crate::worker::{Task, TaskResult};
use crate::Link;
use crate::{address, service};

#[allow(clippy::large_enum_variant)]
/// Control message used internally between workers, users, and the service.
pub enum Control<G: Signer + EcSign> {
    /// Message from the user to the service.
    User(service::Command),
    /// Message from a worker to the service.
    Worker(TaskResult<G>),
}

impl<G: Signer + EcSign> fmt::Debug for Control<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User(cmd) => cmd.fmt(f),
            Self::Worker(resp) => resp.result.fmt(f),
        }
    }
}

/// Peer session type.
pub type WireSession<G> = CypherSession<G, Sha256>;
/// Peer session type (read-only).
pub type WireReader = CypherReader<Sha256>;
/// Peer session type (write-only).
pub type WireWriter<G> = CypherWriter<G, Sha256>;

/// Reactor action.
type Action<G> = reactor::Action<NetAccept<WireSession<G>>, NetTransport<WireSession<G>>>;

/// Peer connection state machine.
enum Peer {
    /// The initial state before handshake is completed.
    Connecting { link: Link, id: Option<NodeId> },
    /// The state after handshake is completed.
    /// Peers in this state are handled by the underlying service.
    Connected { link: Link, id: NodeId },
    /// The state after a peer was disconnected, either during handshake,
    /// or once connected.
    Disconnected {
        id: Option<NodeId>,
        reason: DisconnectReason,
    },
    /// The state after we've started the process of upgraded the peer for a fetch.
    /// The request to handover the socket was made to the reactor.
    Upgrading {
        fetch: Fetch,
        link: Link,
        id: NodeId,
    },
    /// The peer is now upgraded and we are in control of the socket.
    Upgraded { link: Link, id: NodeId },
}

impl std::fmt::Debug for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connecting { link, id: Some(id) } => write!(f, "Connecting({link:?}, {id})"),
            Self::Connecting { link, id: None } => write!(f, "Connecting({link:?})"),
            Self::Connected { link, id } => write!(f, "Connected({link:?}, {id})"),
            Self::Disconnected { .. } => write!(f, "Disconnected"),
            Self::Upgrading { fetch, link, id } => write!(
                f,
                "Upgrading(initiated={}, {link:?}, {id})",
                fetch.initiated
            ),
            Self::Upgraded { link, id, .. } => write!(f, "Upgraded({link:?}, {id})"),
        }
    }
}

impl Peer {
    /// Return the peer's id, if any.
    fn id(&self) -> Option<&NodeId> {
        match self {
            Peer::Connected { id, .. }
            | Peer::Disconnected { id: Some(id), .. }
            | Peer::Upgrading { id, .. }
            | Peer::Upgraded { id, .. } => Some(id),
            _ => None,
        }
    }

    /// Return a new inbound connecting peer.
    fn connecting(link: Link, id: Option<NodeId>) -> Self {
        Self::Connecting { link, id }
    }

    /// Switch to connected state.
    fn connected(&mut self, id: NodeId) {
        if let Self::Connecting { link, .. } = self {
            *self = Self::Connected { link: *link, id };
        } else {
            panic!("Peer::connected: session for {id} is already established");
        }
    }

    /// Switch to disconnected state.
    fn disconnected(&mut self, reason: DisconnectReason) {
        if let Self::Connected { id, .. } = self {
            *self = Self::Disconnected {
                id: Some(*id),
                reason,
            };
        } else if let Self::Connecting { id, .. } = self {
            *self = Self::Disconnected { id: *id, reason };
        } else {
            panic!("Peer::disconnected: session is not connected ({self:?})");
        }
    }

    /// Switch to upgrading state.
    fn upgrading(&mut self, fetch: Fetch) {
        if let Self::Connected { id, link } = self {
            *self = Self::Upgrading {
                fetch,
                id: *id,
                link: *link,
            };
        } else {
            panic!("Peer::upgrading: session is not connected");
        }
    }

    /// Switch to upgraded state.
    fn upgraded(&mut self) -> Fetch {
        if let Self::Upgrading { fetch, id, link } = self {
            let fetch = fetch.clone();
            log::debug!(target: "wire", "Peer {id} upgraded for fetch {}", fetch.rid);

            *self = Self::Upgraded {
                id: *id,
                link: *link,
            };
            fetch
        } else {
            panic!("Peer::upgraded: can't upgrade before handover");
        }
    }

    /// Switch back from upgraded to connected state.
    fn downgrade(&mut self) {
        if let Self::Upgraded { id, link, .. } = self {
            *self = Self::Connected {
                id: *id,
                link: *link,
            };
        } else {
            panic!("Peer::downgrade: can't downgrade if not in upgraded state");
        }
    }
}

/// Wire protocol implementation for a set of peers.
pub struct Wire<R, S, W, G: Signer + EcSign> {
    /// Backing service instance.
    service: Service<R, S, W, G>,
    /// Worker pool interface.
    worker: chan::Sender<Task<G>>,
    /// Used for authentication; keeps local identity.
    cert: Cert<Signature>,
    /// Used for authentication.
    signer: G,
    /// Internal queue of actions to send to the reactor.
    actions: VecDeque<Action<G>>,
    /// Peer sessions.
    peers: HashMap<RawFd, Peer>,
    /// SOCKS5 proxy address.
    proxy: net::SocketAddr,
    /// Buffer for incoming peer data.
    read_queue: VecDeque<u8>,
}

impl<R, S, W, G> Wire<R, S, W, G>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer + EcSign,
{
    pub fn new(
        mut service: Service<R, S, W, G>,
        worker: chan::Sender<Task<G>>,
        cert: Cert<Signature>,
        signer: G,
        proxy: net::SocketAddr,
        clock: LocalTime,
    ) -> Self {
        service
            .initialize(clock)
            .expect("Wire::new: error initializing service");

        Self {
            service,
            worker,
            cert,
            signer,
            proxy,
            actions: VecDeque::new(),
            peers: HashMap::default(),
            read_queue: VecDeque::new(),
        }
    }

    pub fn listen(&mut self, socket: NetAccept<WireSession<G>>) {
        self.actions.push_back(Action::RegisterListener(socket));
    }

    fn peer_mut_by_fd(&mut self, fd: RawFd) -> &mut Peer {
        self.peers.get_mut(&fd).unwrap_or_else(|| {
            log::error!(target: "wire", "Peer with fd {fd} was not found");
            panic!("Peer with fd {fd} is not known");
        })
    }

    fn fd_by_id(&self, node_id: &NodeId) -> (RawFd, &Peer) {
        self.peers
            .iter()
            .find(|(_, peer)| peer.id() == Some(node_id))
            .map(|(fd, peer)| (*fd, peer))
            .unwrap_or_else(|| panic!("Peer {node_id} was expected to be known to the transport"))
    }

    fn connected_fd_by_id(&self, node_id: &NodeId) -> RawFd {
        match self.fd_by_id(node_id) {
            (fd, Peer::Connected { .. }) => fd,
            (fd, peer) => {
                panic!(
                    "Peer {node_id} (fd={fd}) was expected to be in a connected state ({peer:?})"
                )
            }
        }
    }

    fn active(&self) -> impl Iterator<Item = (RawFd, &NodeId)> {
        self.peers.iter().filter_map(|(fd, peer)| match peer {
            Peer::Connecting { id: Some(id), .. } => Some((*fd, id)),
            Peer::Connecting { id: None, .. } => None,
            Peer::Connected { id, .. } => Some((*fd, id)),
            Peer::Upgrading { id, .. } => Some((*fd, id)),
            Peer::Upgraded { id, .. } => Some((*fd, id)),
            Peer::Disconnected { .. } => None,
        })
    }

    fn connected(&self) -> impl Iterator<Item = (RawFd, &NodeId)> {
        self.peers.iter().filter_map(|(fd, peer)| {
            if let Peer::Connected { id, .. } = peer {
                Some((*fd, id))
            } else {
                None
            }
        })
    }

    fn disconnect(&mut self, fd: RawFd, reason: DisconnectReason) {
        let peer = self.peer_mut_by_fd(fd);
        if let Peer::Disconnected { .. } = peer {
            log::error!(target: "wire", "Peer (fd={fd}) is already disconnected");
            return;
        };
        log::debug!(target: "wire", "Disconnecting peer (fd={fd}): {reason}");
        peer.disconnected(reason);

        self.actions.push_back(Action::UnregisterTransport(fd));
    }

    fn upgrade(&mut self, fd: RawFd, fetch: Fetch) {
        let peer = self.peer_mut_by_fd(fd);
        if let Peer::Disconnected { .. } = peer {
            log::error!(target: "wire", "Peer (fd={fd}) is already disconnected");
            return;
        };
        log::debug!(target: "wire", "Requesting transport handover from reactor for peer (fd={fd})");
        peer.upgrading(fetch);

        self.actions.push_back(Action::UnregisterTransport(fd));
    }

    fn upgraded(&mut self, transport: NetTransport<WireSession<G>>) {
        let fd = transport.as_raw_fd();
        let peer = self.peer_mut_by_fd(fd);
        let fetch = peer.upgraded();
        let session = match transport.into_session() {
            Ok(session) => session,
            Err(_) => panic!("Transport::upgraded: peer write buffer not empty on upgrade"),
        };

        if self
            .worker
            .send(Task {
                fetch,
                session,
                drain: self.read_queue.drain(..).collect(),
            })
            .is_err()
        {
            log::error!(target: "wire", "Worker pool is disconnected; cannot send fetch request");
        }
    }

    fn worker_result(&mut self, task: TaskResult<G>) {
        log::debug!(target: "wire", "Fetch completed: {:?}", task.result);

        let session = task.session;
        let fd = session.as_connection().as_raw_fd();
        let peer = self.peer_mut_by_fd(fd);

        let session = if let Peer::Disconnected { .. } = peer {
            log::error!(target: "wire", "Peer with fd {fd} is already disconnected");
            return;
        } else if let Peer::Upgraded { link, .. } = peer {
            match NetTransport::with_session(session, *link) {
                Ok(session) => session,
                Err(err) => {
                    log::error!(target: "wire", "Session downgrade failed: {err}");
                    return;
                }
            }
        } else {
            todo!();
        };
        peer.downgrade();

        self.actions.push_back(Action::RegisterTransport(session));
        self.service.fetched(task.fetch, task.result);
    }
}

impl<R, S, W, G> reactor::Handler for Wire<R, S, W, G>
where
    R: routing::Store + Send,
    S: address::Store + Send,
    W: WriteStorage + Send + 'static,
    G: Signer + EcSign<Pk = NodeId, Sig = Signature> + Clone + Send,
{
    type Listener = NetAccept<WireSession<G>>;
    type Transport = NetTransport<WireSession<G>>;
    type Command = Control<G>;

    fn tick(&mut self, _time: Duration) {
        // FIXME: Change this once a proper timestamp is passed into the function.
        self.service.tick(LocalTime::from(SystemTime::now()));
    }

    fn handle_timer(&mut self) {
        self.service.wake()
    }

    fn handle_listener_event(
        &mut self,
        socket_addr: net::SocketAddr,
        event: ListenerEvent<WireSession<G>>,
        _: Duration,
    ) {
        match event {
            ListenerEvent::Accepted(connection) => {
                log::debug!(
                    target: "wire",
                    "Accepting inbound peer connection from {}..",
                    connection.remote_addr()
                );
                self.peers.insert(
                    connection.as_raw_fd(),
                    Peer::connecting(Link::Inbound, None),
                );

                let session = WireSession::accept::<{ Sha256::OUTPUT_LEN }>(
                    connection,
                    self.cert,
                    vec![],
                    self.signer.clone(),
                );
                let transport = match NetTransport::with_session(session, Link::Inbound) {
                    Ok(transport) => transport,
                    Err(err) => {
                        log::error!(target: "wire", "Failed to create transport for accepted connection: {err}");
                        return;
                    }
                };
                self.service.accepted(socket_addr);
                self.actions
                    .push_back(reactor::Action::RegisterTransport(transport))
            }
            ListenerEvent::Failure(err) => {
                log::error!(target: "wire", "Error listening for inbound connections: {err}");
            }
        }
    }

    fn handle_transport_event(
        &mut self,
        fd: RawFd,
        event: SessionEvent<WireSession<G>>,
        _: Duration,
    ) {
        match event {
            SessionEvent::Established(ProtocolArtifact {
                state: Cert { pk: node_id, .. },
                ..
            }) => {
                log::debug!(target: "wire", "Session established with {node_id} (fd={fd})");

                let conflicting = self
                    .active()
                    .filter(|(other, id)| **id == node_id && *other != fd)
                    .map(|(fd, _)| fd)
                    .collect::<Vec<_>>();

                for fd in conflicting {
                    log::warn!(
                        target: "wire", "Closing conflicting session with {node_id} (fd={fd})"
                    );
                    self.disconnect(
                        fd,
                        DisconnectReason::Dial(Arc::new(io::Error::from(
                            io::ErrorKind::AlreadyExists,
                        ))),
                    );
                }

                let Some(peer) = self.peers.get_mut(&fd) else {
                    log::error!(target: "wire", "Session not found for fd {fd}");
                    return;
                };
                let Peer::Connecting { link, .. } = peer else {
                    log::error!(
                        target: "wire",
                        "Session for {node_id} was either not found, or in an invalid state"
                    );
                    return;
                };
                log::debug!(target: "wire", "Found connecting peer ({:?})..", link);

                let link = *link;

                peer.connected(node_id);
                self.service.connected(node_id, link);
            }
            SessionEvent::Data(data) => {
                if let Some(Peer::Connected { id, .. }) = self.peers.get(&fd) {
                    self.read_queue.extend(data);

                    loop {
                        match Message::decode(&mut self.read_queue) {
                            Ok(msg) => self.service.received_message(*id, msg),
                            Err(err) if err.is_eof() => {
                                // Buffer is empty, or message isn't complete.
                                break;
                            }
                            Err(e) => {
                                log::error!(target: "wire", "Invalid message from {id}: {e}");

                                let mut leftover = if let wire::Error::UnknownMessageType(ty) = e {
                                    ty.to_ne_bytes().to_vec()
                                } else {
                                    vec![]
                                };
                                leftover.extend(self.read_queue.drain(..));

                                if !leftover.is_empty() {
                                    log::debug!(target: "wire", "Dropping read buffer with `{:?}`", &leftover);
                                }
                                self.disconnect(
                                    fd,
                                    // TODO(cloudhead): Include error in reason.
                                    DisconnectReason::Session(session::Error::Misbehavior),
                                );
                                break;
                            }
                        }
                    }
                } else {
                    log::warn!(target: "wire", "Dropping message from unconnected peer with fd {fd}");
                }
            }
            SessionEvent::Terminated(err) => {
                self.disconnect(fd, DisconnectReason::Connection(Arc::new(err)));
            }
        }
    }

    fn handle_command(&mut self, cmd: Self::Command) {
        match cmd {
            Control::User(cmd) => self.service.command(cmd),
            Control::Worker(result) => self.worker_result(result),
        }
    }

    fn handle_error(
        &mut self,
        err: reactor::Error<NetAccept<WireSession<G>>, NetTransport<WireSession<G>>>,
    ) {
        match &err {
            reactor::Error::ListenerUnknown(id) => {
                // TODO: What are we supposed to do here? Remove this error.
                log::error!(target: "wire", "Received error: unknown listener {}", id);
            }
            reactor::Error::TransportUnknown(id) => {
                // TODO: What are we supposed to do here? Remove this error.
                log::error!(target: "wire", "Received error: unknown peer {}", id);
            }
            reactor::Error::Poll(err) => {
                // TODO: This should be a fatal error, there's nothing we can do here.
                log::error!(target: "wire", "Can't poll connections: {}", err);
            }
            reactor::Error::ListenerPollError(id, err) => {
                // TODO: This should be a fatal error, there's nothing we can do here.
                log::error!(target: "wire", "Received error: listener {} disconnected: {}", id, err);
                self.actions.push_back(Action::UnregisterListener(*id));
            }
            reactor::Error::ListenerDisconnect(id, _, err) => {
                // TODO: This should be a fatal error, there's nothing we can do here.
                log::error!(target: "wire", "Received error: listener {} disconnected: {}", id, err);
            }
            reactor::Error::TransportPollError(fd, err) => {
                log::error!(target: "wire", "Received error: peer (fd={fd}) disconnected: {err}");
                self.actions.push_back(Action::UnregisterTransport(*fd));
            }
            reactor::Error::TransportDisconnect(fd, _, err) => {
                log::error!(target: "wire", "Received error: peer (fd={fd}) disconnected: {err}");
            }
            reactor::Error::WriteFailure(id, err) => {
                // TODO: Disconnect peer?
                log::error!(target: "wire", "Error during writing to peer {id}: {err}")
            }
            reactor::Error::WriteLogicError(id, _) => {
                // TODO: We shouldn't be receiving this error. There's nothing we can do.
                log::error!(target: "wire", "Write logic error for peer {id}: {err}")
            }
        }
    }

    fn handover_listener(&mut self, _listener: Self::Listener) {
        panic!("Transport::handover_listener: listener handover is not supported");
    }

    fn handover_transport(&mut self, transport: Self::Transport) {
        let fd = transport.as_raw_fd();
        log::debug!(target: "wire", "Received transport handover (fd={fd})");

        match self.peers.get(&fd) {
            Some(Peer::Disconnected { id, reason }) => {
                // Disconnect TCP stream.
                drop(transport);

                if let Some(id) = id {
                    self.service.disconnected(*id, reason);
                } else {
                    // TODO: Handle this case by calling `disconnected` with the address instead of
                    // the node id.
                }
            }
            Some(Peer::Upgrading { .. }) => {
                self.upgraded(transport);
            }
            Some(_) => {
                panic!("Transport::handover_transport: Unexpected peer with fd {fd} handed over from the reactor");
            }
            None => {
                panic!("Transport::handover_transport: Unknown peer with fd {fd} handed over");
            }
        }
    }
}

impl<R, S, W, G> Iterator for Wire<R, S, W, G>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer + EcSign<Pk = NodeId, Sig = Signature>,
{
    type Item = Action<G>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(ev) = self.service.next() {
            match ev {
                Io::Write(node_id, msgs) => {
                    let fd = match self.fd_by_id(&node_id) {
                        (fd, Peer::Connected { .. }) => fd,
                        (_, peer) => {
                            // If the peer is disconnected by the wire protocol, the service may
                            // not be aware of this yet, and may continue to write messages to it.
                            log::debug!(target: "wire", "Dropping {} message(s) to {node_id} ({peer:?})", msgs.len());
                            continue;
                        }
                    };
                    log::trace!(
                        target: "wire", "Writing {} message(s) to {}", msgs.len(), node_id
                    );

                    let mut data = Vec::new();
                    for msg in msgs {
                        msg.encode(&mut data).expect("in-memory writes never fail");
                    }
                    self.actions.push_back(reactor::Action::Send(fd, data));
                }
                Io::Event(_e) => {
                    log::warn!(
                        target: "wire", "Events are not currently supported"
                    );
                }
                Io::Connect(node_id, addr) => {
                    if self.connected().any(|(_, id)| id == &node_id) {
                        log::error!(
                            target: "wire",
                            "Attempt to connect to already connected peer {node_id}"
                        );
                        break;
                    }

                    match WireSession::connect_nonblocking::<{ Sha256::OUTPUT_LEN }>(
                        addr.to_inner(),
                        self.cert,
                        vec![node_id],
                        self.signer.clone(),
                        self.proxy.into(),
                        false,
                    )
                    .and_then(|session| {
                        NetTransport::<WireSession<G>>::with_session(session, Link::Outbound)
                    }) {
                        Ok(transport) => {
                            self.service.attempted(node_id, &addr);
                            // TODO: Keep track of peer address for when peer disconnects before
                            // handshake is complete.
                            self.peers.insert(
                                transport.as_raw_fd(),
                                Peer::connecting(Link::Outbound, Some(node_id)),
                            );

                            self.actions
                                .push_back(reactor::Action::RegisterTransport(transport));
                        }
                        Err(err) => {
                            log::error!(target: "wire", "Error establishing connection: {err}");

                            self.service
                                .disconnected(node_id, &DisconnectReason::Dial(Arc::new(err)));
                            break;
                        }
                    }
                }
                Io::Disconnect(node_id, reason) => {
                    let fd = self.connected_fd_by_id(&node_id);
                    self.disconnect(fd, reason);
                }
                Io::Wakeup(d) => {
                    self.actions.push_back(reactor::Action::SetTimer(d.into()));
                }
                Io::Fetch(fetch) => {
                    // TODO: Check that the node_id is connected, queue request otherwise.
                    let fd = self.connected_fd_by_id(&fetch.remote);
                    self.upgrade(fd, fetch);
                }
            }
        }
        self.actions.pop_front()
    }
}
