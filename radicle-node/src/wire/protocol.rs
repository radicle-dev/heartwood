//! Implementation of the transport protocol.
//!
//! We use the Noise NN handshake pattern to establish an encrypted stream with a remote peer.
//! The handshake itself is implemented in the external [`cyphernet`] and [`netservices`] crates.
use std::collections::hash_map::Entry;
use std::collections::VecDeque;
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::Arc;
use std::{io, net, time};

use amplify::Wrapper as _;
use crossbeam_channel as chan;
use cyphernet::addr::{HostName, InetHost, NetAddr};
use cyphernet::encrypt::noise::{HandshakePattern, Keyset, NoiseState};
use cyphernet::proxy::socks5;
use cyphernet::{Digest, EcSk, Ecdh, Sha256};
use localtime::LocalTime;
use netservices::resource::{ListenerEvent, NetAccept, NetTransport, SessionEvent};
use netservices::session::{ProtocolArtifact, Socks5Session};
use netservices::{NetConnection, NetProtocol, NetReader, NetWriter};
use reactor::{ResourceId, Timestamp};

use radicle::collections::RandomMap;
use radicle::node::NodeId;
use radicle::storage::WriteStorage;

use crate::crypto::Signer;
use crate::prelude::Deserializer;
use crate::service;
use crate::service::io::Io;
use crate::service::{session, DisconnectReason, Service};
use crate::wire::frame;
use crate::wire::frame::{Frame, FrameData, StreamId};
use crate::wire::Encode;
use crate::worker;
use crate::worker::{ChannelEvent, FetchRequest, FetchResult, Task, TaskResult};
use crate::Link;

/// NoiseXK handshake pattern.
pub const NOISE_XK: HandshakePattern = HandshakePattern {
    initiator: cyphernet::encrypt::noise::InitiatorPattern::Xmitted,
    responder: cyphernet::encrypt::noise::OneWayPattern::Known,
};

/// Default time to wait to receive something from a worker channel. Applies to
/// workers waiting for data from remotes as well.
pub const DEFAULT_CHANNEL_TIMEOUT: time::Duration = time::Duration::from_secs(9);

/// Default time to wait until a network connection is considered inactive.
pub const DEFAULT_CONNECTION_TIMEOUT: time::Duration = time::Duration::from_secs(30);

/// Control message used internally between workers, users, and the service.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Control {
    /// Message from the user to the service.
    User(service::Command),
    /// Message from a worker to the service.
    Worker(TaskResult),
    /// Flush data in the given stream to the remote.
    Flush { remote: NodeId, stream: StreamId },
}

/// Peer session type.
pub type WireSession<G> = NetProtocol<NoiseState<G, Sha256>, Socks5Session<net::TcpStream>>;
/// Peer session type (read-only).
pub type WireReader = NetReader<Socks5Session<net::TcpStream>>;
/// Peer session type (write-only).
pub type WireWriter<G> = NetWriter<NoiseState<G, Sha256>, Socks5Session<net::TcpStream>>;

/// Reactor action.
type Action<G> = reactor::Action<NetAccept<WireSession<G>>, NetTransport<WireSession<G>>>;

/// Streams associated with a connected peer.
struct Streams {
    /// Active streams and their associated worker channels.
    /// Note that the gossip and control streams are not included here as they are always
    /// implied to exist.
    streams: RandomMap<StreamId, worker::Channels>,
    /// Connection direction.
    link: Link,
    /// Sequence number used to compute the next stream id.
    seq: u64,
}

impl Streams {
    /// Create a new [`Streams`] object, passing the connection link.
    fn new(link: Link) -> Self {
        Self {
            streams: RandomMap::default(),
            link,
            seq: 0,
        }
    }

    /// Get a known stream.
    fn get(&self, stream: &StreamId) -> Option<&worker::Channels> {
        self.streams.get(stream)
    }

    /// Open a new stream.
    fn open(&mut self) -> (StreamId, worker::Channels) {
        self.seq += 1;

        let id = StreamId::git(self.link)
            .nth(self.seq)
            .expect("Streams::open: too many streams");
        let channels = self
            .register(id)
            .expect("Streams::open: stream was already open");

        (id, channels)
    }

    /// Register an open stream.
    fn register(&mut self, stream: StreamId) -> Option<worker::Channels> {
        let (wire, worker) = worker::Channels::pair(DEFAULT_CHANNEL_TIMEOUT)
            .expect("Streams::register: fatal: unable to create channels");

        match self.streams.entry(stream) {
            Entry::Vacant(e) => {
                e.insert(worker);
                Some(wire)
            }
            Entry::Occupied(_) => None,
        }
    }

    /// Unregister an open stream.
    fn unregister(&mut self, stream: &StreamId) -> Option<worker::Channels> {
        self.streams.remove(stream)
    }

    /// Close all streams.
    fn shutdown(&mut self) {
        for (sid, chans) in self.streams.drain() {
            log::debug!(target: "wire", "Closing worker stream {sid}");
            chans.close().ok();
        }
    }
}

/// The initial state of an outbound peer before handshake is completed.
#[derive(Debug)]
struct Outbound {
    /// Resource ID, if registered.
    id: Option<ResourceId>,
    /// Remote address.
    addr: NetAddr<HostName>,
    /// Remote Node ID.
    nid: NodeId,
}

/// The initial state of an inbound peer before handshake is completed.
#[derive(Debug)]
struct Inbound {
    /// Resource ID, if registered.
    id: Option<ResourceId>,
    /// Remote address.
    addr: NetAddr<HostName>,
}

/// Peer connection state machine.
enum Peer {
    /// The state after handshake is completed.
    /// Peers in this state are handled by the underlying service.
    Connected {
        #[allow(dead_code)]
        addr: NetAddr<HostName>,
        link: Link,
        nid: NodeId,
        inbox: Deserializer<Frame>,
        streams: Streams,
    },
    /// The peer was scheduled for disconnection. Once the transport is handed over
    /// by the reactor, we can consider it disconnected.
    Disconnecting {
        nid: Option<NodeId>,
        reason: DisconnectReason,
    },
}

impl std::fmt::Debug for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connected { link, nid, .. } => write!(f, "Connected({link:?}, {nid})"),
            Self::Disconnecting { .. } => write!(f, "Disconnecting"),
        }
    }
}

impl Peer {
    /// Return the peer's id, if any.
    fn id(&self) -> Option<&NodeId> {
        match self {
            Peer::Connected { nid, .. } | Peer::Disconnecting { nid: Some(nid), .. } => Some(nid),
            Peer::Disconnecting { nid: None, .. } => None,
        }
    }

    /// Connected peer.
    fn connected(nid: NodeId, addr: NetAddr<HostName>, link: Link) -> Self {
        Self::Connected {
            link,
            addr,
            nid,
            inbox: Deserializer::default(),
            streams: Streams::new(link),
        }
    }

    /// Switch to disconnecting state.
    fn disconnecting(&mut self, reason: DisconnectReason) {
        if let Self::Connected { nid, streams, .. } = self {
            streams.shutdown();

            *self = Self::Disconnecting {
                nid: Some(*nid),
                reason,
            };
        } else {
            panic!("Peer::disconnected: session is not connected ({self:?})");
        }
    }
}

/// Holds connected peers.
struct Peers(RandomMap<ResourceId, Peer>);

impl Peers {
    fn get_mut(&mut self, id: &ResourceId) -> Option<&mut Peer> {
        self.0.get_mut(id)
    }

    fn entry(&mut self, id: ResourceId) -> Entry<ResourceId, Peer> {
        self.0.entry(id)
    }

    fn insert(&mut self, id: ResourceId, peer: Peer) {
        if self.0.insert(id, peer).is_some() {
            log::warn!(target: "wire", "Replacing existing peer id={id}");
        }
    }

    fn remove(&mut self, id: &ResourceId) -> Option<Peer> {
        self.0.remove(id)
    }

    fn lookup(&self, node_id: &NodeId) -> Option<(ResourceId, &Peer)> {
        self.0
            .iter()
            .find(|(_, peer)| peer.id() == Some(node_id))
            .map(|(fd, peer)| (*fd, peer))
    }

    fn lookup_mut(&mut self, node_id: &NodeId) -> Option<(ResourceId, &mut Peer)> {
        self.0
            .iter_mut()
            .find(|(_, peer)| peer.id() == Some(node_id))
            .map(|(fd, peer)| (*fd, peer))
    }

    fn active(&self) -> impl Iterator<Item = (ResourceId, &NodeId)> {
        self.0.iter().filter_map(|(id, peer)| match peer {
            Peer::Connected { nid, .. } => Some((*id, nid)),
            Peer::Disconnecting { .. } => None,
        })
    }

    fn connected(&self) -> impl Iterator<Item = (ResourceId, &NodeId)> {
        self.0.iter().filter_map(|(id, peer)| {
            if let Peer::Connected { nid, .. } = peer {
                Some((*id, nid))
            } else {
                None
            }
        })
    }
}

/// Wire protocol implementation for a set of peers.
pub struct Wire<D, S, G: Signer + Ecdh> {
    /// Backing service instance.
    service: Service<D, S, G>,
    /// Worker pool interface.
    worker: chan::Sender<Task>,
    /// Used for authentication.
    signer: G,
    /// Internal queue of actions to send to the reactor.
    actions: VecDeque<Action<G>>,
    /// Outbound attempted peers without a session.
    outbound: RandomMap<RawFd, Outbound>,
    /// Inbound peers without a session.
    inbound: RandomMap<RawFd, Inbound>,
    /// Peer (established) sessions.
    peers: Peers,
    /// SOCKS5 proxy address.
    proxy: net::SocketAddr,
}

impl<D, S, G> Wire<D, S, G>
where
    D: service::Store,
    S: WriteStorage + 'static,
    G: Signer + Ecdh<Pk = NodeId>,
{
    pub fn new(
        service: Service<D, S, G>,
        worker: chan::Sender<Task>,
        signer: G,
        proxy: net::SocketAddr,
    ) -> Self {
        assert!(service.started().is_some(), "Service must be initialized");

        Self {
            service,
            worker,
            signer,
            proxy,
            actions: VecDeque::new(),
            inbound: RandomMap::default(),
            outbound: RandomMap::default(),
            peers: Peers(RandomMap::default()),
        }
    }

    pub fn listen(&mut self, socket: NetAccept<WireSession<G>>) {
        self.actions.push_back(Action::RegisterListener(socket));
    }

    fn disconnect(&mut self, id: ResourceId, reason: DisconnectReason) {
        match self.peers.get_mut(&id) {
            Some(Peer::Disconnecting { .. }) => {
                log::error!(target: "wire", "Peer with id={id} is already disconnecting");
            }
            Some(peer) => {
                log::debug!(target: "wire", "Disconnecting peer with id={id}: {reason}");

                peer.disconnecting(reason);
                self.actions.push_back(Action::UnregisterTransport(id));
            }
            None => {
                // Connecting peer with no session.
                log::debug!(target: "wire", "Disconnecting pending peer with id={id}: {reason}");
                self.actions.push_back(Action::UnregisterTransport(id));
            }
        }
    }

    fn worker_result(&mut self, task: TaskResult) {
        log::trace!(
            target: "wire",
            "Received fetch result from worker for stream {}, remote {}: {:?}",
            task.stream, task.remote, task.result
        );

        let nid = task.remote;
        let Some((fd, peer)) = self.peers.lookup_mut(&nid) else {
            log::warn!(target: "wire", "Peer {nid} not found; ignoring fetch result");
            return;
        };

        let Peer::Connected {
            nid, link, streams, ..
        } = peer
        else {
            log::warn!(target: "wire", "Peer {nid} is not connected; ignoring fetch result");
            return;
        };

        // Only call into the service if we initiated this fetch.
        match task.result {
            FetchResult::Initiator { rid, result } => {
                self.service.fetched(rid, *nid, result);
            }
            FetchResult::Responder { .. } => {
                // We don't do anything with upload results for now.
            }
        }

        // Nb. It's possible that the stream would already be unregistered if we received an early
        // "close" from the remote. Otherwise, we unregister it here and send the "close" ourselves.
        if streams.unregister(&task.stream).is_some() {
            let frame = Frame::control(
                *link,
                frame::Control::Close {
                    stream: task.stream,
                },
            );
            self.actions.push_back(Action::Send(fd, frame.to_bytes()));
        }
    }

    fn flush(&mut self, remote: NodeId, stream: StreamId) {
        let Some((fd, peer)) = self.peers.lookup(&remote) else {
            log::warn!(target: "wire", "Peer {remote} is not known; ignoring flush");
            return;
        };
        let Peer::Connected { streams, link, .. } = peer else {
            log::warn!(target: "wire", "Peer {remote} is not connected; ignoring flush");
            return;
        };
        let Some(c) = streams.get(&stream) else {
            log::debug!(target: "wire", "Stream {stream} cannot be found; ignoring flush");
            return;
        };

        for data in c.try_iter() {
            let frame = match data {
                ChannelEvent::Data(data) => Frame::git(stream, data),
                ChannelEvent::Close => Frame::control(*link, frame::Control::Close { stream }),
                ChannelEvent::Eof => Frame::control(*link, frame::Control::Eof { stream }),
            };
            self.actions
                .push_back(reactor::Action::Send(fd, frame.to_bytes()));
        }
    }

    fn cleanup(&mut self, id: ResourceId, fd: RawFd) {
        if self.inbound.remove(&fd).is_some() {
            log::debug!(target: "wire", "Cleaning up inbound peer state with id={id} (fd={fd})");
        } else if let Some(outbound) = self.outbound.remove(&fd) {
            log::debug!(target: "wire", "Cleaning up outbound peer state with id={id} (fd={fd})");
            self.service
                .disconnected(outbound.nid, &DisconnectReason::connection());
        } else {
            log::warn!(target: "wire", "Tried to cleanup unknown peer with id={id} (fd={fd})");
        }
    }
}

impl<D, S, G> reactor::Handler for Wire<D, S, G>
where
    D: service::Store + Send,
    S: WriteStorage + Send + 'static,
    G: Signer + Ecdh<Pk = NodeId> + Clone + Send,
{
    type Listener = NetAccept<WireSession<G>>;
    type Transport = NetTransport<WireSession<G>>;
    type Command = Control;

    fn tick(&mut self, time: Timestamp) {
        self.service
            .tick(LocalTime::from_millis(time.as_millis() as u128));
    }

    fn handle_timer(&mut self) {
        self.service.wake();
    }

    fn handle_listener_event(
        &mut self,
        _: ResourceId, // Nb. This is the ID of the listener socket.
        event: ListenerEvent<WireSession<G>>,
        _: Timestamp,
    ) {
        match event {
            ListenerEvent::Accepted(connection) => {
                let addr = connection.remote_addr();
                let fd = connection.as_raw_fd();
                log::debug!(target: "wire", "Accepting inbound connection from {addr} (fd={fd})..");

                // If the service doesn't want to accept this connection,
                // we drop the connection here, which disconnects the socket.
                if !self.service.accepted(NetAddr::from(addr.clone()).into()) {
                    log::debug!(target: "wire", "Rejecting inbound connection from {addr} (fd={fd})..");
                    drop(connection);

                    return;
                }

                let session = accept::<G>(connection, self.signer.clone());
                let transport = match NetTransport::with_session(session, Link::Inbound) {
                    Ok(transport) => transport,
                    Err(err) => {
                        log::error!(target: "wire", "Failed to create transport for accepted connection: {err}");
                        return;
                    }
                };

                self.inbound.insert(
                    fd,
                    Inbound {
                        id: None,
                        addr: addr.into(),
                    },
                );
                self.actions
                    .push_back(reactor::Action::RegisterTransport(transport))
            }
            ListenerEvent::Failure(err) => {
                log::error!(target: "wire", "Error listening for inbound connections: {err}");
            }
        }
    }

    fn handle_registered(&mut self, fd: RawFd, id: ResourceId) {
        if let Some(outbound) = self.outbound.get_mut(&fd) {
            log::debug!(target: "wire", "Outbound peer resource registered for {} with id={id} (fd={fd})", outbound.nid);
            outbound.id = Some(id);
        } else if let Some(inbound) = self.inbound.get_mut(&fd) {
            log::debug!(target: "wire", "Inbound peer resource registered with id={id} (fd={fd})");
            inbound.id = Some(id);
        } else {
            log::warn!(target: "wire", "Unknown peer registered with fd={fd} and id={id}");
        }
    }

    fn handle_transport_event(
        &mut self,
        id: ResourceId,
        event: SessionEvent<WireSession<G>>,
        _: Timestamp,
    ) {
        match event {
            SessionEvent::Established(fd, ProtocolArtifact { state, .. }) => {
                // SAFETY: With the NoiseXK protocol, there is always a remote static key.
                let nid: NodeId = state.remote_static_key.unwrap();
                // Make sure we don't try to connect to ourselves by mistake.
                if &nid == self.signer.public_key() {
                    log::error!(target: "wire", "Self-connection detected, disconnecting..");

                    self.disconnect(
                        id,
                        DisconnectReason::Dial(Arc::new(io::Error::from(
                            io::ErrorKind::AlreadyExists,
                        ))),
                    );
                    return;
                }
                log::debug!(target: "wire", "Session established with {nid} (id={id}) (fd={fd})");

                let conflicting = self
                    .peers
                    .active()
                    .filter(|(other, d)| **d == nid && *other != id)
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>();

                for id in conflicting {
                    log::warn!(
                        target: "wire", "Closing conflicting session with {nid} (id={id})"
                    );
                    self.disconnect(
                        id,
                        DisconnectReason::Dial(Arc::new(io::Error::from(
                            io::ErrorKind::AlreadyExists,
                        ))),
                    );
                }

                let (addr, link) = if let Some(peer) = self.inbound.remove(&fd) {
                    (peer.addr, Link::Inbound)
                } else if let Some(peer) = self.outbound.remove(&fd) {
                    assert_eq!(nid, peer.nid);
                    (peer.addr, Link::Outbound)
                } else {
                    log::error!(target: "wire", "Session for {nid} (id={id}) not found");
                    return;
                };
                self.peers
                    .insert(id, Peer::connected(nid, addr.clone(), link));
                self.service.connected(nid, addr.into(), link);
            }
            SessionEvent::Data(data) => {
                if let Some(Peer::Connected {
                    nid,
                    inbox,
                    streams,
                    ..
                }) = self.peers.get_mut(&id)
                {
                    inbox.input(&data);

                    loop {
                        match inbox.deserialize_next() {
                            Ok(Some(Frame {
                                data: FrameData::Control(frame::Control::Open { stream }),
                                ..
                            })) => {
                                log::debug!(target: "wire", "Received `open` command for stream {stream} from {nid}");

                                let Some(channels) = streams.register(stream) else {
                                    log::warn!(target: "wire", "Peer attempted to open already-open stream stream {stream}");
                                    continue;
                                };

                                let task = Task {
                                    fetch: FetchRequest::Responder { remote: *nid },
                                    stream,
                                    channels,
                                };
                                if self.worker.send(task).is_err() {
                                    log::error!(target: "wire", "Worker pool is disconnected; cannot send task");
                                }
                            }
                            Ok(Some(Frame {
                                data: FrameData::Control(frame::Control::Eof { stream }),
                                ..
                            })) => {
                                if let Some(channels) = streams.get(&stream) {
                                    log::debug!(target: "wire", "Received `end-of-file` on stream {stream} from {nid}");

                                    if channels.send(ChannelEvent::Eof).is_err() {
                                        log::error!(target: "wire", "Worker is disconnected; cannot send `EOF`");
                                    }
                                } else {
                                    log::debug!(target: "wire", "Ignoring frame on closed or unknown stream {stream}");
                                }
                            }
                            Ok(Some(Frame {
                                data: FrameData::Control(frame::Control::Close { stream }),
                                ..
                            })) => {
                                log::debug!(target: "wire", "Received `close` command for stream {stream} from {nid}");

                                if let Some(chans) = streams.unregister(&stream) {
                                    chans.close().ok();
                                }
                            }
                            Ok(Some(Frame {
                                data: FrameData::Gossip(msg),
                                ..
                            })) => {
                                self.service.received_message(*nid, msg);
                            }
                            Ok(Some(Frame {
                                stream,
                                data: FrameData::Git(data),
                                ..
                            })) => {
                                if let Some(channels) = streams.get(&stream) {
                                    if channels.send(ChannelEvent::Data(data)).is_err() {
                                        log::error!(target: "wire", "Worker is disconnected; cannot send data");
                                    }
                                } else {
                                    log::debug!(target: "wire", "Ignoring frame on closed or unknown stream {stream}");
                                }
                            }
                            Ok(None) => {
                                // Buffer is empty, or message isn't complete.
                                break;
                            }
                            Err(e) => {
                                log::error!(target: "wire", "Invalid gossip message from {nid}: {e}");

                                if !inbox.is_empty() {
                                    log::debug!(target: "wire", "Dropping read buffer for {nid} with {} bytes", inbox.unparsed().count());
                                }
                                self.disconnect(
                                    id,
                                    DisconnectReason::Session(session::Error::Misbehavior),
                                );
                                break;
                            }
                        }
                    }
                } else {
                    log::warn!(target: "wire", "Dropping message from unconnected peer (id={id})");
                }
            }
            SessionEvent::Terminated(err) => {
                self.disconnect(id, DisconnectReason::Connection(Arc::new(err)));
            }
        }
    }

    fn handle_command(&mut self, cmd: Self::Command) {
        match cmd {
            Control::User(cmd) => self.service.command(cmd),
            Control::Worker(result) => self.worker_result(result),
            Control::Flush { remote, stream } => self.flush(remote, stream),
        }
    }

    fn handle_error(
        &mut self,
        err: reactor::Error<NetAccept<WireSession<G>>, NetTransport<WireSession<G>>>,
    ) {
        match err {
            reactor::Error::Poll(err) => {
                // TODO: This should be a fatal error, there's nothing we can do here.
                log::error!(target: "wire", "Can't poll connections: {err}");
            }
            reactor::Error::ListenerDisconnect(id, _) => {
                // TODO: This should be a fatal error, there's nothing we can do here.
                log::error!(target: "wire", "Listener {id} disconnected");
            }
            reactor::Error::TransportDisconnect(id, transport) => {
                let fd = transport.as_raw_fd();
                log::error!(target: "wire", "Peer id={id} (fd={fd}) disconnected");

                // We're dropping the TCP connection here.
                drop(transport);

                // The peer transport is already disconnected and removed from the reactor;
                // therefore there is no need to initiate a disconnection. We simply remove
                // the peer from the map.
                match self.peers.remove(&id) {
                    Some(mut peer) => {
                        if let Peer::Connected { streams, .. } = &mut peer {
                            streams.shutdown();
                        }

                        if let Some(id) = peer.id() {
                            self.service
                                .disconnected(*id, &DisconnectReason::connection());
                        } else {
                            log::debug!(target: "wire", "Inbound disconnection before handshake; ignoring..")
                        }
                    }
                    None => self.cleanup(id, fd),
                }
            }
        }
    }

    fn handover_listener(&mut self, id: ResourceId, _listener: Self::Listener) {
        log::error!(target: "wire", "Listener handover is not supported (id={id})");
    }

    fn handover_transport(&mut self, id: ResourceId, transport: Self::Transport) {
        let fd = transport.as_raw_fd();

        match self.peers.entry(id) {
            Entry::Occupied(e) => {
                match e.get() {
                    Peer::Disconnecting { nid, reason, .. } => {
                        log::debug!(target: "wire", "Transport handover for disconnecting peer with id={id} (fd={fd})");

                        // Disconnect TCP stream.
                        drop(transport);

                        // If there is no NID, the service is not aware of the peer.
                        if let Some(nid) = nid {
                            self.service.disconnected(*nid, reason);
                        }
                        e.remove();
                    }
                    Peer::Connected { nid, .. } => {
                        panic!("Wire::handover_transport: Unexpected handover of connected peer {} with id={id} (fd={fd})", nid);
                    }
                }
            }
            Entry::Vacant(_) => self.cleanup(id, fd),
        }
    }
}

impl<D, S, G> Iterator for Wire<D, S, G>
where
    D: service::Store,
    S: WriteStorage + 'static,
    G: Signer + Ecdh<Pk = NodeId>,
{
    type Item = Action<G>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(ev) = self.service.next() {
            match ev {
                Io::Write(node_id, msgs) => {
                    let (fd, link) = match self.peers.lookup(&node_id) {
                        Some((fd, Peer::Connected { link, .. })) => (fd, *link),
                        Some((_, peer)) => {
                            // If the peer is disconnected by the wire protocol, the service may
                            // not be aware of this yet, and may continue to write messages to it.
                            log::debug!(target: "wire", "Dropping {} message(s) to {node_id} ({peer:?})", msgs.len());
                            continue;
                        }
                        None => {
                            log::error!(target: "wire", "Dropping {} message(s) to {node_id}: unknown peer", msgs.len());
                            continue;
                        }
                    };
                    log::trace!(
                        target: "wire", "Writing {} message(s) to {}", msgs.len(), node_id
                    );

                    let mut data = Vec::new();
                    for msg in msgs {
                        Frame::gossip(link, msg)
                            .encode(&mut data)
                            .expect("in-memory writes never fail");
                    }
                    self.actions.push_back(reactor::Action::Send(fd, data));
                }
                Io::Connect(node_id, addr) => {
                    if self.peers.connected().any(|(_, id)| id == &node_id) {
                        log::error!(
                            target: "wire",
                            "Attempt to connect to already connected peer {node_id}"
                        );
                        continue;
                    }
                    self.service.attempted(node_id, addr.clone());

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
                            log::debug!(
                                target: "wire",
                                "Registering transport for {node_id} (fd={})..",
                                transport.as_raw_fd()
                            );
                            self.outbound.insert(
                                transport.as_raw_fd(),
                                Outbound {
                                    id: None,
                                    nid: node_id,
                                    addr: addr.to_inner(),
                                },
                            );
                            self.actions
                                .push_back(reactor::Action::RegisterTransport(transport));
                        }
                        Err(err) => {
                            log::error!(target: "wire", "Error establishing connection to {addr}: {err}");

                            self.service
                                .disconnected(node_id, &DisconnectReason::Dial(Arc::new(err)));
                        }
                    }
                }
                Io::Disconnect(nid, reason) => {
                    if let Some((id, Peer::Connected { .. })) = self.peers.lookup(&nid) {
                        self.disconnect(id, reason);
                    } else {
                        log::warn!(target: "wire", "Peer {nid} is not connected: ignoring disconnect");
                    }
                }
                Io::Wakeup(d) => {
                    self.actions.push_back(reactor::Action::SetTimer(
                        // TODO: Remove this when `io-reactor` can handle `0` duration timeouts.
                        d.max(localtime::LocalDuration::from_millis(1)).into(),
                    ));
                }
                Io::Fetch {
                    rid,
                    remote,
                    timeout,
                    refs_at,
                    ..
                } => {
                    log::trace!(target: "wire", "Processing fetch for {rid} from {remote}..");

                    let Some((fd, Peer::Connected { link, streams, .. })) =
                        self.peers.lookup_mut(&remote)
                    else {
                        // Nb. It's possible that a peer is disconnected while an `Io::Fetch`
                        // is in the service's i/o buffer. Since the service may not purge the
                        // buffer on disconnect, we should just ignore i/o actions that don't
                        // have a connected peer.
                        log::error!(target: "wire", "Peer {remote} is not connected: dropping fetch");
                        continue;
                    };
                    let (stream, channels) = streams.open();

                    log::debug!(target: "wire", "Opened new stream with id {stream} for {rid} and remote {remote}");

                    let link = *link;
                    let task = Task {
                        fetch: FetchRequest::Initiator {
                            rid,
                            remote,
                            refs_at,
                            timeout,
                        },
                        stream,
                        channels,
                    };

                    if !self.worker.is_empty() {
                        log::warn!(
                            target: "wire",
                            "Worker pool is busy: {} tasks pending, fetch requests may be delayed", self.worker.len()
                        );
                    }
                    if self.worker.send(task).is_err() {
                        log::error!(target: "wire", "Worker pool is disconnected; cannot send fetch request");
                    }
                    self.actions.push_back(Action::Send(
                        fd,
                        Frame::control(link, frame::Control::Open { stream }).to_bytes(),
                    ));
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
        net::TcpStream::connect_nonblocking(proxy_addr, DEFAULT_CONNECTION_TIMEOUT)?
    } else {
        net::TcpStream::connect_nonblocking(
            remote_addr.connection_addr(proxy_addr),
            DEFAULT_CONNECTION_TIMEOUT,
        )?
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::service::{Message, ZeroBytes};
    use crate::wire;
    use crate::wire::varint;

    #[test]
    fn test_message_with_extension() {
        use crate::deserializer;

        let mut stream = Vec::new();
        let pong = Message::Pong {
            zeroes: ZeroBytes::new(42),
        };
        frame::PROTOCOL_VERSION.encode(&mut stream).unwrap();
        frame::StreamId::gossip(Link::Outbound)
            .encode(&mut stream)
            .unwrap();

        // Serialize gossip message with some extension fields.
        let mut gossip = wire::serialize(&pong);
        String::from("extra").encode(&mut gossip).unwrap();
        48u8.encode(&mut gossip).unwrap();

        // Encode gossip message using the varint-prefix format into the stream.
        varint::payload::encode(&gossip, &mut stream).unwrap();

        let mut de = deserializer::Deserializer::<Frame>::new(1024);
        de.input(&stream);

        // The "pong" message decodes successfully, even though there is trailing data.
        assert_eq!(
            de.deserialize_next().unwrap().unwrap(),
            Frame::gossip(Link::Outbound, pong)
        );
        assert!(de.deserialize_next().unwrap().is_none());
        assert!(de.is_empty());
    }
}
