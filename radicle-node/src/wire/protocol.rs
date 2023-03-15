//! Implementation of the transport protocol.
//!
//! We use the Noise NN handshake pattern to establish an encrypted stream with a remote peer.
//! The handshake itself is implemented in the external [`cyphernet`] and [`netservices`] crates.
use std::collections::hash_map::Entry;
use std::collections::VecDeque;
use std::os::unix::io::AsRawFd;
use std::os::unix::prelude::RawFd;
use std::sync::Arc;
use std::{fmt, io, net};

use amplify::Wrapper as _;
use crossbeam_channel as chan;
use cyphernet::addr::{HostName, InetHost, NetAddr};
use cyphernet::encrypt::noise::{HandshakePattern, Keyset, NoiseState};
use cyphernet::proxy::socks5;
use cyphernet::{Digest, EcSk, Ecdh, Sha256};
use localtime::LocalTime;
use netservices::resource::{ListenerEvent, NetAccept, NetTransport, SessionEvent};
use netservices::session::{ProtocolArtifact, Socks5Session};
use netservices::{NetConnection, NetProtocol, NetReader, NetSession, NetWriter};
use reactor::Timestamp;

use radicle::collections::HashMap;
use radicle::node::{routing, NodeId};
use radicle::storage::WriteStorage;

use crate::crypto::Signer;
use crate::service::reactor::{Fetch, Io};
use crate::service::{self, session, DisconnectReason, Message, Service};
use crate::wire::{self, Decode, Encode};
use crate::worker::{Task, TaskResult};
use crate::{address, Link};

/// NoiseXK handshake pattern.
pub const NOISE_XK: HandshakePattern = HandshakePattern {
    initiator: cyphernet::encrypt::noise::InitiatorPattern::Xmitted,
    responder: cyphernet::encrypt::noise::OneWayPattern::Known,
};

#[allow(clippy::large_enum_variant)]
/// Control message used internally between workers, users, and the service.
pub enum Control<G: Signer + Ecdh> {
    /// Message from the user to the service.
    User(service::Command),
    /// Message from a worker to the service.
    Worker(TaskResult<G>),
}

impl<G: Signer + Ecdh> fmt::Debug for Control<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User(cmd) => cmd.fmt(f),
            Self::Worker(resp) => resp.result.fmt(f),
        }
    }
}

/// Peer session type.
pub type WireSession<G> = NetProtocol<NoiseState<G, Sha256>, Socks5Session<net::TcpStream>>;
/// Peer session type (read-only).
pub type WireReader = NetReader<Socks5Session<net::TcpStream>>;
/// Peer session type (write-only).
pub type WireWriter<G> = NetWriter<NoiseState<G, Sha256>, Socks5Session<net::TcpStream>>;

/// Reactor action.
type Action<G> = reactor::Action<NetAccept<WireSession<G>>, NetTransport<WireSession<G>>>;

/// Peer connection state machine.
enum Peer {
    /// The initial state of an inbound peer before handshake is completed.
    Inbound {},
    /// The initial state of an outbound peer before handshake is completed.
    Outbound { id: NodeId },
    /// The state after handshake is completed.
    /// Peers in this state are handled by the underlying service.
    Connected {
        link: Link,
        id: NodeId,
        inbox: VecDeque<u8>,
    },
    /// The peer was scheduled for disconnection. Once the transport is handed over
    /// by the reactor, we can consider it disconnected.
    Disconnecting {
        id: Option<NodeId>,
        reason: DisconnectReason,
    },
    /// The state after we've started the process of upgraded the peer for a fetch.
    /// The request to handover the socket was made to the reactor.
    Upgrading {
        fetch: Fetch,
        link: Link,
        id: NodeId,
        inbox: VecDeque<u8>,
    },
    /// The peer is now upgraded and we are in control of the socket.
    Upgraded { link: Link, id: NodeId },
}

impl std::fmt::Debug for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inbound {} => write!(f, "Inbound"),
            Self::Outbound { id } => write!(f, "Outbound({id})"),
            Self::Connected { link, id, .. } => write!(f, "Connected({link:?}, {id})"),
            Self::Disconnecting { .. } => write!(f, "Disconnecting"),
            Self::Upgrading {
                fetch, link, id, ..
            } => write!(
                f,
                "Upgrading(initiated={}, {link:?}, {id})",
                fetch.is_initiator(),
            ),
            Self::Upgraded { link, id, .. } => write!(f, "Upgraded({link:?}, {id})"),
        }
    }
}

impl Peer {
    /// Return the peer's id, if any.
    fn id(&self) -> Option<&NodeId> {
        match self {
            Peer::Outbound { id }
            | Peer::Connected { id, .. }
            | Peer::Disconnecting { id: Some(id), .. }
            | Peer::Upgrading { id, .. }
            | Peer::Upgraded { id, .. } => Some(id),

            Peer::Inbound {} => None,
            Peer::Disconnecting { id: None, .. } => None,
        }
    }

    /// Return a new inbound connecting peer.
    fn inbound() -> Self {
        Self::Inbound {}
    }

    /// Return a new inbound connecting peer.
    fn outbound(id: NodeId) -> Self {
        Self::Outbound { id }
    }

    /// Switch to connected state.
    fn connected(&mut self, id: NodeId) -> Link {
        if let Self::Inbound {} = self {
            let link = Link::Inbound;

            *self = Self::Connected {
                link,
                id,
                inbox: VecDeque::new(),
            };
            link
        } else if let Self::Outbound { id: expected } = self {
            assert_eq!(id, *expected);
            let link = Link::Outbound;

            *self = Self::Connected {
                link,
                id,
                inbox: VecDeque::new(),
            };
            link
        } else {
            panic!("Peer::connected: session for {id} is already established");
        }
    }

    /// Switch to disconnecting state.
    fn disconnecting(&mut self, reason: DisconnectReason) {
        if let Self::Connected { id, .. } = self {
            *self = Self::Disconnecting {
                id: Some(*id),
                reason,
            };
        } else if let Self::Inbound {} = self {
            *self = Self::Disconnecting { id: None, reason };
        } else if let Self::Outbound { id } = self {
            *self = Self::Disconnecting {
                id: Some(*id),
                reason,
            };
        } else {
            panic!("Peer::disconnected: session is not connected ({self:?})");
        }
    }

    /// Switch to upgrading state.
    fn upgrading(&mut self, fetch: Fetch) {
        if let Self::Connected { id, link, inbox } = self {
            *self = Self::Upgrading {
                fetch,
                id: *id,
                link: *link,
                inbox: inbox.clone(),
            };
        } else {
            panic!("Peer::upgrading: session is not fully connected");
        }
    }

    /// Switch to upgraded state. Returns the unread bytes from the peer.
    #[must_use]
    fn upgraded(&mut self) -> (Fetch, Vec<u8>) {
        if let Self::Upgrading {
            fetch,
            id,
            link,
            inbox,
        } = self
        {
            let fetch = fetch.clone();
            let inbox = inbox.drain(..).collect();
            log::debug!(target: "wire", "Peer {id} upgraded for fetch {}", fetch.rid);

            *self = Self::Upgraded {
                id: *id,
                link: *link,
            };
            (fetch, inbox)
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
                inbox: VecDeque::new(),
            };
        } else {
            panic!("Peer::downgrade: can't downgrade if not in upgraded state");
        }
    }
}

/// Wire protocol implementation for a set of peers.
pub struct Wire<R, S, W, G: Signer + Ecdh> {
    /// Backing service instance.
    service: Service<R, S, W, G>,
    /// Worker pool interface.
    worker: chan::Sender<Task<G>>,
    /// Used for authentication.
    signer: G,
    /// Internal queue of actions to send to the reactor.
    actions: VecDeque<Action<G>>,
    /// Peer sessions.
    peers: HashMap<RawFd, Peer>,
    /// SOCKS5 proxy address.
    proxy: net::SocketAddr,
}

impl<R, S, W, G> Wire<R, S, W, G>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer + Ecdh<Pk = NodeId>,
{
    pub fn new(
        mut service: Service<R, S, W, G>,
        worker: chan::Sender<Task<G>>,
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
            signer,
            proxy,
            actions: VecDeque::new(),
            peers: HashMap::default(),
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
            Peer::Inbound {} => None,
            Peer::Outbound { id } => Some((*fd, id)),
            Peer::Connected { id, .. } => Some((*fd, id)),
            Peer::Upgrading { id, .. } => Some((*fd, id)),
            Peer::Upgraded { id, .. } => Some((*fd, id)),
            Peer::Disconnecting { .. } => None,
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
        match self.peers.get_mut(&fd) {
            Some(Peer::Disconnecting { .. }) => {
                log::error!(target: "wire", "Peer (fd={fd}) is already disconnecting");
            }
            Some(peer) => {
                log::debug!(target: "wire", "Disconnecting peer (fd={fd}): {reason}");

                peer.disconnecting(reason);
                self.actions.push_back(Action::UnregisterTransport(fd));
            }
            None => {
                log::error!(target: "wire", "Unknown peer (fd={fd}) cannot be disconnected");
            }
        }
    }

    fn upgrade(&mut self, fd: RawFd, fetch: Fetch) {
        let peer = self.peer_mut_by_fd(fd);
        if let Peer::Disconnecting { .. } = peer {
            log::error!(target: "wire", "Peer (fd={fd}) is disconnecting");
            return;
        };
        log::debug!(target: "wire", "Requesting transport handover from reactor for peer (fd={fd})");
        peer.upgrading(fetch);

        self.actions.push_back(Action::UnregisterTransport(fd));
    }

    fn upgraded(&mut self, transport: NetTransport<WireSession<G>>) {
        let fd = transport.as_raw_fd();
        let peer = self.peer_mut_by_fd(fd);
        let (fetch, drain) = peer.upgraded();
        let session = match transport.into_session() {
            Ok(session) => session,
            Err(_) => panic!("Transport::upgraded: peer write buffer not empty on upgrade"),
        };

        if self
            .worker
            .send(Task {
                fetch,
                session,
                drain,
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

        let session = if let Peer::Disconnecting { .. } = peer {
            log::error!(target: "wire", "Peer with fd {fd} is disconnecting");
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
    G: Signer + Ecdh<Pk = NodeId> + Clone + Send,
{
    type Listener = NetAccept<WireSession<G>>;
    type Transport = NetTransport<WireSession<G>>;
    type Command = Control<G>;

    fn tick(&mut self, time: Timestamp) {
        self.service
            .tick(LocalTime::from_millis(time.as_millis() as u128));
    }

    fn handle_timer(&mut self) {
        self.service.wake();
    }

    fn handle_listener_event(
        &mut self,
        socket_addr: net::SocketAddr,
        event: ListenerEvent<WireSession<G>>,
        _: Timestamp,
    ) {
        match event {
            ListenerEvent::Accepted(connection) => {
                log::debug!(
                    target: "wire",
                    "Accepting inbound peer connection from {}..",
                    connection.remote_addr()
                );
                self.peers.insert(connection.as_raw_fd(), Peer::inbound());

                let session = accept::<G>(connection, self.signer.clone());
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
        _: Timestamp,
    ) {
        match event {
            SessionEvent::Established(ProtocolArtifact { state, .. }) => {
                // SAFETY: With the NoiseXK protocol, there is always a remote static key.
                let id: NodeId = state.remote_static_key.unwrap();

                log::debug!(target: "wire", "Session established with {id} (fd={fd})");

                let conflicting = self
                    .active()
                    .filter(|(other, d)| **d == id && *other != fd)
                    .map(|(fd, _)| fd)
                    .collect::<Vec<_>>();

                for fd in conflicting {
                    log::warn!(
                        target: "wire", "Closing conflicting session with {id} (fd={fd})"
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
                let link = peer.connected(id);

                self.service.connected(id, link);
            }
            SessionEvent::Data(data) => {
                if let Some(Peer::Connected { id, inbox, .. }) = self.peers.get_mut(&fd) {
                    inbox.extend(data);

                    loop {
                        match Message::decode(inbox) {
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
                                leftover.extend(inbox.drain(..));

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
                } else if let Some(Peer::Upgrading { inbox, .. }) = self.peers.get_mut(&fd) {
                    // If somehow the remote peer managed to send git data before the reactor
                    // unregistered our session, we'll hit this branch.
                    inbox.extend(data);
                } else {
                    log::warn!(target: "wire", "Dropping message from unconnected peer (fd={fd})");
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
            reactor::Error::ListenerPollError(id, _) => {
                // TODO: This should be a fatal error, there's nothing we can do here.
                log::error!(target: "wire", "Received error: listener {} disconnected", id);
                self.actions.push_back(Action::UnregisterListener(*id));
            }
            reactor::Error::ListenerDisconnect(id, _, _) => {
                // TODO: This should be a fatal error, there's nothing we can do here.
                log::error!(target: "wire", "Received error: listener {} disconnected", id);
            }
            reactor::Error::TransportPollError(fd, _) => {
                log::error!(target: "wire", "Received error: peer (fd={fd}) poll error");
                self.actions.push_back(Action::UnregisterTransport(*fd));
            }
            reactor::Error::TransportDisconnect(fd, _, _) => {
                log::error!(target: "wire", "Received error: peer (fd={fd}) disconnected");

                // The peer transport is already disconnected and removed from the reactor;
                // therefore there is no need to initiate a disconnection. We simply remove
                // the peer from the map.
                match self.peers.remove(fd) {
                    Some(peer) => {
                        let reason = DisconnectReason::Connection(Arc::new(io::Error::from(
                            io::ErrorKind::ConnectionReset,
                        )));

                        if let Some(id) = peer.id() {
                            self.service.disconnected(*id, &reason);
                        } else {
                            log::debug!(target: "wire", "Inbound disconnection before handshake; ignoring..")
                        }
                    }
                    None => {
                        log::warn!(target: "wire", "Peer with fd {fd} is unknown");
                    }
                }
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
        panic!("Wire::handover_listener: listener handover is not supported");
    }

    fn handover_transport(&mut self, transport: Self::Transport) {
        let fd = transport.as_raw_fd();
        log::debug!(target: "wire", "Received transport handover (fd={fd})");

        match self.peers.entry(fd) {
            Entry::Occupied(e) => {
                match e.get() {
                    Peer::Disconnecting { id, reason, .. } => {
                        // Disconnect TCP stream.
                        drop(transport);

                        // If there is no ID, the service is not aware of the peer.
                        if let Some(id) = id {
                            self.service.disconnected(*id, reason);
                        }
                        e.remove();
                    }
                    Peer::Upgrading { .. } => {
                        self.upgraded(transport);
                    }
                    _ => {
                        panic!("Wire::handover_transport: Unexpected peer with fd {fd} handed over from the reactor");
                    }
                }
            }
            Entry::Vacant(_) => {
                panic!("Wire::handover_transport: Unknown peer with fd {fd} handed over");
            }
        }
    }
}

impl<R, S, W, G> Iterator for Wire<R, S, W, G>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer + Ecdh<Pk = NodeId>,
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

                    match dial::<G>(
                        addr.to_inner(),
                        node_id,
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
                            self.peers
                                .insert(transport.as_raw_fd(), Peer::outbound(node_id));

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

/// Establish a new outgoing connection.
pub fn dial<G: Signer + Ecdh<Pk = NodeId>>(
    remote_addr: NetAddr<HostName>,
    remote_id: <G as EcSk>::Pk,
    signer: G,
    proxy_addr: NetAddr<InetHost>,
    force_proxy: bool,
) -> io::Result<WireSession<G>> {
    let connection = if force_proxy {
        net::TcpStream::connect_nonblocking(proxy_addr)?
    } else {
        net::TcpStream::connect_nonblocking(remote_addr.connection_addr(proxy_addr))?
    };
    Ok(session::<G>(
        remote_addr,
        Some(remote_id),
        connection,
        signer,
        force_proxy,
    ))
}

/// Accept a new connection.
pub fn accept<G: Signer + Ecdh<Pk = NodeId>>(
    connection: net::TcpStream,
    signer: G,
) -> WireSession<G> {
    session::<G>(
        connection.remote_addr().into(),
        None,
        connection,
        signer,
        false,
    )
}

/// Create a new [`WireSession`].
fn session<G: Signer + Ecdh<Pk = NodeId>>(
    remote_addr: NetAddr<HostName>,
    remote_id: Option<NodeId>,
    connection: net::TcpStream,
    signer: G,
    force_proxy: bool,
) -> WireSession<G> {
    let socks5 = socks5::Socks5::with(remote_addr, force_proxy);
    let proxy = Socks5Session::with(connection, socks5);
    let pair = G::generate_keypair();
    let keyset = Keyset {
        e: pair.0,
        s: Some(signer),
        re: None,
        rs: remote_id,
    };

    let noise = NoiseState::initialize::<{ Sha256::OUTPUT_LEN }>(
        NOISE_XK,
        remote_id.is_some(),
        &[],
        keyset,
    );
    WireSession::with(proxy, noise)
}
