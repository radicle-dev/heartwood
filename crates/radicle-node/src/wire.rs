//! Implementation of the transport protocol.
//!
//! We use the Noise XK handshake pattern to establish an encrypted stream with a remote peer.
use std::collections::hash_map::Entry;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::sync::Arc;
use std::{io, net, time};

use crossbeam_channel as chan;
use cyphernet::addr::{HostName, InetHost, NetAddr};
use cyphernet::encrypt::noise::{HandshakePattern, Keyset, NoiseState};
use cyphernet::proxy::socks5;
use cyphernet::{Digest, EcSk, Ecdh, Sha256};
use localtime::LocalTime;
use mio::net::TcpStream;
use radicle::node::device::Device;

use radicle::collections::{RandomMap, RandomSet};
use radicle::crypto;
use radicle::node::config::AddressConfig;
use radicle::node::Link;
use radicle::node::NodeId;
use radicle::storage::WriteStorage;
use radicle_protocol::deserializer::Deserializer;
pub use radicle_protocol::wire::frame;
pub use radicle_protocol::wire::frame::{Frame, FrameData, StreamId};
pub use radicle_protocol::wire::*;
use radicle_protocol::worker::{FetchRequest, FetchResult};

use crate::reactor;
use crate::reactor::{Listener, Transport};
use crate::reactor::{NoiseSession, ProtocolArtifact, SessionEvent, Socks5Session};
use crate::reactor::{Token, Tokens};
use crate::service;
use crate::service::io::Io;
use crate::service::FETCH_TIMEOUT;
use crate::service::{session, DisconnectReason, Metrics, Service};
use crate::worker;
use crate::worker::{ChannelEvent, ChannelsConfig};
use crate::worker::{Task, TaskResult};

/// NoiseXK handshake pattern.
pub const NOISE_XK: HandshakePattern = HandshakePattern {
    initiator: cyphernet::encrypt::noise::InitiatorPattern::Xmitted,
    responder: cyphernet::encrypt::noise::OneWayPattern::Known,
};

/// Default time to wait until a network connection is considered inactive.
pub const DEFAULT_CONNECTION_TIMEOUT: time::Duration = time::Duration::from_secs(6);

/// Maximum size of a peer inbox, in bytes.
pub const MAX_INBOX_SIZE: usize = 1024 * 1024 * 2;

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
type WireSession<G> = NoiseSession<G, Sha256, Socks5Session<TcpStream>>;

/// Reactor action.
type Action<G> = reactor::Action<Listener, Transport<WireSession<G>>>;

/// A worker stream.
struct Stream {
    /// Channels.
    channels: worker::Channels,
    /// Data sent.
    sent_bytes: usize,
    /// Data received.
    received_bytes: usize,
}

impl Stream {
    fn new(channels: worker::Channels) -> Self {
        Self {
            channels,
            sent_bytes: 0,
            received_bytes: 0,
        }
    }
}

/// Streams associated with a connected peer.
struct Streams {
    /// Active streams and their associated worker channels.
    /// Note that the gossip and control streams are not included here as they are always
    /// implied to exist.
    streams: RandomMap<StreamId, Stream>,
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
    fn get(&self, stream: &StreamId) -> Option<&Stream> {
        self.streams.get(stream)
    }

    /// Get a known stream, mutably.
    fn get_mut(&mut self, stream: &StreamId) -> Option<&mut Stream> {
        self.streams.get_mut(stream)
    }

    /// Open a new stream.
    fn open(&mut self, config: ChannelsConfig) -> (StreamId, worker::Channels) {
        self.seq += 1;

        let id = StreamId::git(self.link)
            .nth(self.seq)
            .expect("Streams::open: too many streams");
        let channels = self
            .register(id, config)
            .expect("Streams::open: stream was already open");

        (id, channels)
    }

    /// Register an open stream.
    fn register(&mut self, stream: StreamId, config: ChannelsConfig) -> Option<worker::Channels> {
        let (wire, worker) = worker::Channels::pair(config)
            .expect("Streams::register: fatal: unable to create channels");

        match self.streams.entry(stream) {
            Entry::Vacant(e) => {
                e.insert(Stream::new(worker));
                Some(wire)
            }
            Entry::Occupied(_) => None,
        }
    }

    /// Unregister an open stream.
    fn unregister(&mut self, stream: &StreamId) -> Option<Stream> {
        self.streams.remove(stream)
    }

    /// Close all streams.
    fn shutdown(&mut self) {
        for (sid, stream) in self.streams.drain() {
            log::debug!(target: "wire", "Closing worker stream {sid}");
            stream.channels.close().ok();
        }
    }
}

/// The initial state of an outbound peer before handshake is completed.
#[derive(Debug)]
struct Outbound {
    /// Token for I/O event notification.
    token: Token,
    /// Remote address.
    addr: NetAddr<HostName>,
    /// Remote Node ID.
    nid: NodeId,
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
        inbox: Deserializer<MAX_INBOX_SIZE, Frame>,
        streams: Streams,
    },
    /// The peer was scheduled for disconnection. Once the transport is handed over
    /// by the reactor, we can consider it disconnected.
    Disconnecting {
        link: Link,
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

    fn link(&self) -> Link {
        match self {
            Peer::Connected { link, .. } => *link,
            Peer::Disconnecting { link, .. } => *link,
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
}

/// Holds connected peers.
struct Peers(RandomMap<Token, Peer>);

impl Peers {
    fn get_mut(&mut self, token: &Token) -> Option<&mut Peer> {
        self.0.get_mut(token)
    }

    fn entry(&mut self, token: Token) -> Entry<'_, Token, Peer> {
        self.0.entry(token)
    }

    fn insert(&mut self, token: Token, peer: Peer) {
        if self.0.insert(token, peer).is_some() {
            log::warn!(target: "wire", token=token.0; "Replacing existing peer");
        }
    }

    fn remove(&mut self, id: &Token) -> Option<Peer> {
        self.0.remove(id)
    }

    fn lookup(&self, id: &NodeId) -> Option<(Token, &Peer)> {
        self.0
            .iter()
            .find(|(_, peer)| peer.id() == Some(id))
            .map(|(token, peer)| (*token, peer))
    }

    fn lookup_mut(&mut self, id: &NodeId) -> Option<(Token, &mut Peer)> {
        self.0
            .iter_mut()
            .find(|(_, peer)| peer.id() == Some(id))
            .map(|(fd, peer)| (*fd, peer))
    }

    fn active(&self) -> impl Iterator<Item = (Token, &NodeId, Link)> {
        self.0.iter().filter_map(|(id, peer)| match peer {
            Peer::Connected { nid, link, .. } => Some((*id, nid, *link)),
            Peer::Disconnecting { .. } => None,
        })
    }

    fn connected(&self) -> impl Iterator<Item = (Token, &NodeId)> {
        self.0.iter().filter_map(|(id, peer)| {
            if let Peer::Connected { nid, .. } = peer {
                Some((*id, nid))
            } else {
                None
            }
        })
    }

    fn iter(&self) -> impl Iterator<Item = &Peer> {
        self.0.values()
    }
}

/// Wire protocol implementation for a set of peers.
pub(crate) struct Wire<D, S, G: crypto::signature::Signer<crypto::Signature> + Ecdh> {
    /// Backing service instance.
    service: Service<D, S, G>,
    /// Worker pool interface.
    worker: chan::Sender<Task>,
    /// Used for authentication.
    signer: Device<G>,
    /// Node metrics.
    metrics: service::Metrics,
    /// Internal queue of actions to send to the reactor.
    actions: VecDeque<Action<G>>,
    /// Outbound attempted peers without a session.
    outbound: RandomMap<Token, Outbound>,
    /// Inbound peers without a session.
    inbound: RandomSet<Token>,
    /// Listening addresses that are not yet registered.
    listening: RandomMap<Token, net::SocketAddr>,
    /// Peer (established) sessions.
    peers: Peers,
    /// A (practically) infinite source of tokens to identify transports and listeners.
    tokens: Tokens,
}

impl<D, S, G> Wire<D, S, G>
where
    D: service::Store,
    S: WriteStorage + 'static,
    G: crypto::signature::Signer<crypto::Signature> + Ecdh<Pk = NodeId>,
{
    pub fn new(service: Service<D, S, G>, worker: chan::Sender<Task>, signer: Device<G>) -> Self {
        assert!(service.started().is_some(), "Service must be initialized");

        Self {
            service,
            worker,
            signer,
            metrics: Metrics::default(),
            actions: VecDeque::new(),
            inbound: RandomSet::default(),
            outbound: RandomMap::default(),
            listening: RandomMap::default(),
            peers: Peers(RandomMap::default()),
            tokens: Tokens::default(),
        }
    }

    pub fn listen(&mut self, socket: Listener) {
        let token = self.tokens.advance();
        self.listening.insert(token, socket.local_addr());
        self.actions
            .push_back(Action::RegisterListener(token, socket));
    }

    fn disconnect(&mut self, token: Token, reason: DisconnectReason) -> Option<(NodeId, Link)> {
        match self.peers.entry(token) {
            Entry::Vacant(_) => {
                // Connecting peer with no session.
                log::debug!(target: "wire", token=token.0; "Disconnecting pending peer: {reason}");
                self.actions.push_back(Action::UnregisterTransport(token));

                // Check for attempted outbound connections. Unestablished inbound connections don't
                // have an NID yet.
                self.outbound
                    .values()
                    .find(|o| o.token == token)
                    .map(|o| (o.nid, Link::Outbound))
            }
            Entry::Occupied(mut e) => match e.get_mut() {
                Peer::Disconnecting { nid, link, .. } => {
                    log::error!(target: "wire", token=token.0; "Peer is already disconnecting");

                    nid.map(|n| (n, *link))
                }
                Peer::Connected {
                    nid, streams, link, ..
                } => {
                    log::debug!(target: "wire", token=token.0; "Disconnecting peer: {reason}");
                    let nid = *nid;
                    let link = *link;

                    streams.shutdown();
                    e.insert(Peer::Disconnecting {
                        nid: Some(nid),
                        link,
                        reason,
                    });
                    self.actions.push_back(Action::UnregisterTransport(token));

                    Some((nid, link))
                }
            },
        }
    }

    fn worker_result(&mut self, task: TaskResult) {
        log::debug!(
            target: "wire",
            "Received fetch result from worker for stream {}, remote {}: {:?}",
            task.stream, task.remote, task.result
        );

        let nid = task.remote;
        let Some((fd, peer)) = self.peers.lookup_mut(&nid) else {
            log::warn!(target: "wire", "Peer {nid} not found; ignoring fetch result");
            return;
        };

        if let Peer::Connected { link, streams, .. } = peer {
            // Nb. It's possible that the stream would already be unregistered if we received an
            // early "close" from the remote. Otherwise, we unregister it here and send the "close"
            // ourselves.
            if let Some(s) = streams.unregister(&task.stream) {
                log::debug!(
                    target: "wire", "Stream {} of {} closing with {} byte(s) sent and {} byte(s) received",
                    task.stream, task.remote, s.sent_bytes, s.received_bytes
                );
                let frame = Frame::<service::Message>::control(
                    *link,
                    frame::Control::Close {
                        stream: task.stream,
                    },
                );
                self.actions
                    .push_back(Action::Send(fd, frame.encode_to_vec()));
            }
        } else {
            // If the peer disconnected, we'll get here, but we still want to let the service know
            // about the fetch result, so we don't return here.
            log::warn!(target: "wire", "Peer {nid} is not connected; ignoring fetch result");
            return;
        };

        // Only call into the service if we initiated this fetch.
        match task.result {
            FetchResult::Initiator { rid, result } => {
                self.service.fetched(rid, nid, result);
            }
            FetchResult::Responder { rid, result } => {
                if let Some(rid) = rid {
                    if let Some(err) = result.err() {
                        log::info!(target: "wire", "Peer {nid} failed to fetch {rid} from us: {err}");
                    } else {
                        log::info!(target: "wire", "Peer {nid} fetched {rid} from us successfully");
                    }
                }
            }
        }
    }

    fn flush(&mut self, remote: NodeId, stream: StreamId) {
        let Some((fd, peer)) = self.peers.lookup_mut(&remote) else {
            log::warn!(target: "wire", "Peer {remote} is not known; ignoring flush");
            return;
        };
        let Peer::Connected { streams, link, .. } = peer else {
            log::warn!(target: "wire", "Peer {remote} is not connected; ignoring flush");
            return;
        };
        let Some(s) = streams.get_mut(&stream) else {
            log::debug!(target: "wire", "Stream {stream} cannot be found; ignoring flush");
            return;
        };
        let metrics = self.metrics.peer(remote);

        for data in s.channels.try_iter() {
            let frame = match data {
                ChannelEvent::Data(data) => {
                    metrics.sent_git_bytes += data.len();
                    metrics.sent_bytes += data.len();
                    Frame::<service::Message>::git(stream, data)
                }
                ChannelEvent::Close => Frame::control(*link, frame::Control::Close { stream }),
                ChannelEvent::Eof => Frame::control(*link, frame::Control::Eof { stream }),
            };
            self.actions
                .push_back(reactor::Action::Send(fd, frame.encode_to_vec()));
        }
    }

    fn cleanup(&mut self, token: Token) {
        if self.inbound.remove(&token) {
            log::debug!(target: "wire", token=token.0; "Cleaning up inbound peer state");
        } else if let Some(outbound) = self.outbound.remove(&token) {
            log::debug!(target: "wire", token=token.0; "Cleaning up outbound peer state");
            self.service.disconnected(
                outbound.nid,
                Link::Outbound,
                &DisconnectReason::connection(),
            );
        } else {
            log::debug!(target: "wire", token=token.0; "Tried to cleanup unknown peer");
        }
    }
}

impl<D, S, G> reactor::ReactionHandler for Wire<D, S, G>
where
    D: service::Store + Send,
    S: WriteStorage + Send + 'static,
    G: crypto::signature::Signer<crypto::Signature> + Ecdh<Pk = NodeId> + Clone + Send + Debug,
{
    type Listener = Listener;
    type Transport = Transport<WireSession<G>>;

    fn tick(&mut self, time: LocalTime) {
        self.metrics.open_channels = self
            .peers
            .iter()
            .filter_map(|p| {
                if let Peer::Connected { streams, .. } = p {
                    Some(streams.streams.len())
                } else {
                    None
                }
            })
            .sum();
        self.metrics.worker_queue_size = self.worker.len();
        self.service.tick(
            LocalTime::from_millis(time.as_millis() as u128),
            &self.metrics,
        );
    }

    fn timer_reacted(&mut self) {
        self.service.wake();
    }

    fn listener_reacted(
        &mut self,
        _: Token, // Note that this is the token of the listener socket.
        event: io::Result<(TcpStream, std::net::SocketAddr)>,
        _: LocalTime,
    ) {
        match event {
            Ok((connection, peer)) => {
                let remote = NetAddr::from(peer);
                let InetHost::Ip(ip) = remote.host else {
                    log::error!(target: "wire", "Unexpected host type for inbound connection {remote}; dropping..");
                    drop(connection);

                    return;
                };
                log::debug!(target: "wire", "Inbound connection from {remote}..");

                // If the service doesn't want to accept this connection,
                // we drop the connection here, which disconnects the socket.
                if !self.service.accepted(ip) {
                    log::debug!(target: "wire", "Rejecting inbound connection from {ip}..");
                    drop(connection);

                    return;
                }

                let session = accept::<G>(
                    remote.clone().into(),
                    connection,
                    self.signer.clone().into_inner(),
                );
                let transport = match Transport::with_session(session, Link::Inbound) {
                    Ok(transport) => transport,
                    Err(err) => {
                        log::error!(target: "wire", "Failed to create transport for accepted connection: {err}");
                        return;
                    }
                };

                let token = self.tokens.advance();
                log::debug!(target: "wire", token=token.0; "Accepted inbound connection from {remote}..");

                self.inbound.insert(token);
                self.actions
                    .push_back(reactor::Action::RegisterTransport(token, transport))
            }
            Err(err) => {
                log::error!(target: "wire", "Error listening for inbound connections: {err}");
            }
        }
    }

    fn listener_registered(&mut self, token: Token, _listener: &Self::Listener) {
        if let Some(local_addr) = self.listening.remove(&token) {
            self.service.listening(local_addr);
        }
    }

    fn transport_registered(&mut self, token: Token, _transport: &Self::Transport) {
        if let Some(outbound) = self.outbound.get_mut(&token) {
            log::debug!(target: "wire", token=token.0; "Outbound peer resource registered for {}", outbound.nid);
        } else if self.inbound.contains(&token) {
            log::debug!(target: "wire", token=token.0; "Inbound peer resource registered");
        } else {
            log::warn!(target: "wire", token=token.0; "Unknown peer registered");
        }
    }

    fn transport_reacted(
        &mut self,
        token: Token,
        event: SessionEvent<WireSession<G>>,
        _: LocalTime,
    ) {
        match event {
            SessionEvent::Established(ProtocolArtifact { state, session }) => {
                // SAFETY: With the NoiseXK protocol, there is always a remote static key.
                let nid: NodeId = state.remote_static_key.unwrap();
                // Make sure we don't try to connect to ourselves by mistake.
                if &nid == self.signer.public_key() {
                    log::error!(target: "wire", "Self-connection detected, disconnecting..");
                    self.disconnect(token, DisconnectReason::SelfConnection);

                    return;
                }

                let established_addr: NetAddr<HostName> = session.state;
                let (addr, link) = if self.inbound.remove(&token) {
                    self.metrics.peer(nid).inbound_connection_attempts += 1;
                    (established_addr, Link::Inbound)
                } else if let Some(peer) = self.outbound.remove(&token) {
                    assert_eq!(nid, peer.nid);
                    (peer.addr, Link::Outbound)
                } else {
                    log::error!(target: "wire", token=token.0; "Session for {nid} not found");
                    return;
                };
                log::debug!(
                    target: "wire", token=token.0, direction:display=link; "Session established with {nid}"
                );

                // Connections to close.
                let mut disconnect = Vec::new();

                // Handle conflicting connections.
                // This is typical when users have mutually configured their nodes to connect to
                // each other on startup. We handle this by deterministically choosing one node
                // whose outbound connection is the one that is kept. The other connections are
                // dropped.
                {
                    // Having precedence means that our outbound connection will win over
                    // the other node's outbound connection.
                    enum Precedence {
                        Ours,
                        Theirs,
                    }

                    use Link::*;
                    use Precedence::*;

                    // Whether we have precedence in case of conflicting connections.
                    let precedence = if *self.signer.public_key() > nid {
                        Ours
                    } else {
                        Theirs
                    };

                    // Active sessions with the same NID but a different token are conflicting.
                    let peers = self.peers.active().filter_map(|(c_id, d, link)| {
                        (*d == nid && c_id != token).then_some((c_id, link))
                    });

                    // Outbound connection attempts with the same remote key but a different file
                    // descriptor are conflicting.
                    let outbound = self.outbound.iter().filter_map(|(c_id, other)| {
                        (other.nid == nid && *c_id != token).then_some((*c_id, Outbound))
                    });

                    for (c_token, c_link) in peers.chain(outbound) {
                        // If we have precedence, the inbound connection is closed.
                        // In the case where both connections are inbound or outbound,
                        // we close the newer connection, ie. the one with the higher
                        // token.
                        let close = match (link, c_link, &precedence) {
                            (Inbound, Outbound, Ours) => token,
                            (Inbound, Outbound, Theirs) => c_token,
                            (Outbound, Inbound, Ours) => c_token,
                            (Outbound, Inbound, Theirs) => token,
                            (Inbound, Inbound, _) => token.max(c_token),
                            (Outbound, Outbound, _) => token.max(c_token),
                        };

                        log::warn!(
                            target: "wire", "Established session with token {} conflicts with existing session with token {} for {nid}. Disconnecting session with token {}.", token.0, c_token.0, close.0
                        );
                        disconnect.push(close);
                    }
                }
                for id in &disconnect {
                    log::warn!(
                        target: "wire", token=token.0; "Closing conflicting session with {nid}.."
                    );
                    // Disconnect and return the associated NID of the peer, if available.
                    if let Some((nid, link)) = self.disconnect(*id, DisconnectReason::Conflict) {
                        // We disconnect the session eagerly because otherwise we will get the new
                        // `connected` event before the `disconnect`, resulting in a duplicate
                        // connection.
                        self.service
                            .disconnected(nid, link, &DisconnectReason::Conflict);
                    }
                }
                if !disconnect.contains(&token) {
                    self.peers
                        .insert(token, Peer::connected(nid, addr.clone(), link));
                    self.service.connected(nid, addr.into(), link);
                }
            }
            SessionEvent::Data(data) => {
                if let Some(Peer::Connected {
                    nid,
                    inbox,
                    streams,
                    ..
                }) = self.peers.get_mut(&token)
                {
                    let metrics = self.metrics.peer(*nid);
                    metrics.received_bytes += data.len();

                    if inbox.input(&data).is_err() {
                        log::error!(target: "wire", "Maximum inbox size ({MAX_INBOX_SIZE}) reached for peer {nid}");
                        log::error!(target: "wire", "Unable to process messages fast enough for peer {nid}; disconnecting..");
                        self.disconnect(
                            token,
                            DisconnectReason::Session(session::Error::Misbehavior),
                        );

                        return;
                    }

                    loop {
                        match inbox.deserialize_next() {
                            Ok(Some(Frame {
                                data: FrameData::Control(frame::Control::Open { stream }),
                                ..
                            })) => {
                                log::debug!(target: "wire", "Received `open` command for stream {stream} from {nid}");
                                metrics.streams_opened += 1;
                                metrics.received_fetch_requests += 1;
                                let reader_limit = self.service.config().limits.fetch_pack_receive;
                                let Some(channels) = streams.register(
                                    stream,
                                    ChannelsConfig::new(FETCH_TIMEOUT)
                                        .with_reader_limit(reader_limit),
                                ) else {
                                    log::warn!(target: "wire", "Peer attempted to open already-open stream stream {stream}");
                                    continue;
                                };

                                let task = Task {
                                    fetch: FetchRequest::Responder {
                                        remote: *nid,
                                        emitter: self.service.emitter(),
                                    },
                                    stream,
                                    channels,
                                };
                                if let Err(e) = self.worker.try_send(task) {
                                    log::error!(
                                        target: "wire",
                                        "Worker pool failed to accept incoming fetch request: {e}"
                                    );
                                }
                            }
                            Ok(Some(Frame {
                                data: FrameData::Control(frame::Control::Eof { stream }),
                                ..
                            })) => {
                                if let Some(s) = streams.get(&stream) {
                                    log::debug!(target: "wire", "Received `end-of-file` on stream {stream} from {nid}");

                                    if s.channels.send(ChannelEvent::Eof).is_err() {
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

                                if let Some(s) = streams.unregister(&stream) {
                                    log::debug!(
                                        target: "wire",
                                        "Stream {stream} of {nid} closed with {} byte(s) sent and {} byte(s) received",
                                        s.sent_bytes, s.received_bytes
                                    );
                                    s.channels.close().ok();
                                }
                            }
                            Ok(Some(Frame {
                                data: FrameData::Gossip(msg),
                                ..
                            })) => {
                                metrics.received_gossip_messages += 1;
                                self.service.received_message(*nid, msg);
                            }
                            Ok(Some(Frame {
                                stream,
                                data: FrameData::Git(data),
                                ..
                            })) => {
                                if let Some(s) = streams.get_mut(&stream) {
                                    metrics.received_git_bytes += data.len();

                                    if s.channels.send(ChannelEvent::Data(data)).is_err() {
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
                                    log::debug!(target: "wire", "Dropping read buffer for {nid} with {} bytes", inbox.len());
                                }
                                self.disconnect(
                                    token,
                                    DisconnectReason::Session(session::Error::Misbehavior),
                                );
                                break;
                            }
                        }
                    }
                } else {
                    log::warn!(target: "wire", token=token.0; "Dropping message from unconnected peer");
                }
            }
            SessionEvent::Terminated(err) => {
                self.disconnect(token, DisconnectReason::Connection(Arc::new(err)));
            }
        }
    }

    fn handle_command(&mut self, cmd: Control) {
        match cmd {
            Control::User(cmd) => self.service.command(cmd),
            Control::Worker(result) => self.worker_result(result),
            Control::Flush { remote, stream } => self.flush(remote, stream),
        }
    }

    fn handle_error(&mut self, err: reactor::Error<Listener, Transport<WireSession<G>>>) {
        match err {
            reactor::Error::Poll(err) | reactor::Error::Registration(err) => {
                // TODO: This should be a fatal error, there's nothing we can do here.
                log::error!(target: "wire", "Can't poll connections: {err}");
            }
            reactor::Error::ListenerDisconnect(token, _) => {
                // TODO: This should be a fatal error, there's nothing we can do here.
                log::error!(target: "wire", token=token.0; "Listener disconnected");
            }
            reactor::Error::TransportDisconnect(token, transport) => {
                log::error!(target: "wire", token=token.0; "Peer disconnected");

                // We're dropping the TCP connection here.
                drop(transport);

                // The peer transport is already disconnected and removed from the reactor;
                // therefore there is no need to initiate a disconnection. We simply remove
                // the peer from the map.
                match self.peers.remove(&token) {
                    Some(mut peer) => {
                        if let Peer::Connected { streams, .. } = &mut peer {
                            streams.shutdown();
                        }

                        if let Some(id) = peer.id() {
                            self.service.disconnected(
                                *id,
                                peer.link(),
                                &DisconnectReason::connection(),
                            );
                        } else {
                            log::debug!(target: "wire", "Inbound disconnection before handshake; ignoring..")
                        }
                    }
                    None => self.cleanup(token),
                }
            }
        }
    }

    fn handover_listener(&mut self, token: Token, _listener: Self::Listener) {
        log::error!(target: "wire", token=token.0; "Listener handover is not supported");
    }

    fn handover_transport(&mut self, token: Token, transport: Self::Transport) {
        match self.peers.entry(token) {
            Entry::Occupied(e) => {
                match e.get() {
                    Peer::Disconnecting {
                        nid, reason, link, ..
                    } => {
                        log::debug!(target: "wire", token=token.0; "Transport handover for disconnecting peer");

                        // Disconnect TCP stream.
                        drop(transport);

                        // If there is no NID, the service is not aware of the peer.
                        if let Some(nid) = nid {
                            // In the case of a conflicting connection, there will be two resources
                            // for the peer. However, at the service level, there is only one, and
                            // it is identified by NID.
                            //
                            // Therefore, we specify which of the connections we're closing by
                            // passing the `link`.
                            self.service.disconnected(*nid, *link, reason);
                        }
                        e.remove();
                    }
                    Peer::Connected { nid, .. } => {
                        panic!("Wire::handover_transport: Unexpected handover of connected peer {nid} with token {}", token.0);
                    }
                }
            }
            Entry::Vacant(_) => self.cleanup(token),
        }
    }
}

impl<D, S, G> Iterator for Wire<D, S, G>
where
    D: service::Store,
    S: WriteStorage + 'static,
    G: crypto::signature::Signer<crypto::Signature> + Ecdh<Pk = NodeId> + Clone,
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
                    let metrics = self.metrics.peer(node_id);
                    metrics.sent_gossip_messages += msgs.len();

                    for msg in msgs {
                        Frame::gossip(link, msg).encode(&mut data);
                    }
                    metrics.sent_bytes += data.len();

                    self.actions.push_back(reactor::Action::Send(fd, data));
                }
                Io::Connect(node_id, addr) => {
                    if self.peers.connected().any(|(_, id)| id == &node_id) {
                        log::error!(
                            target: "wire",
                            "Attempt to connect to already connected peer {node_id}"
                        );
                        // FIXME: The problem here is the session will stay in "initial" state,
                        // because it can't transition to attempted.
                        continue;
                    }
                    self.service.attempted(node_id, addr.clone());
                    self.metrics.peer(node_id).outbound_connection_attempts += 1;

                    match dial::<G>(
                        (*addr).clone(),
                        node_id,
                        self.signer.clone().into_inner(),
                        self.service.config(),
                    )
                    .and_then(|session| {
                        Transport::<WireSession<G>>::with_session(session, Link::Outbound)
                    }) {
                        Ok(transport) => {
                            let token = self.tokens.advance();
                            self.outbound.insert(
                                token,
                                Outbound {
                                    token,
                                    nid: node_id,
                                    addr: (*addr).clone(),
                                },
                            );
                            log::debug!(
                                target: "wire",
                                "Registering outbound transport for {node_id}.."
                            );
                            self.actions
                                .push_back(reactor::Action::RegisterTransport(token, transport));
                        }
                        Err(err) => {
                            log::error!(target: "wire", "Error establishing connection to {addr}: {err}");

                            self.service.disconnected(
                                node_id,
                                Link::Outbound,
                                &DisconnectReason::Dial(Arc::new(err)),
                            );
                        }
                    }
                }
                Io::Disconnect(nid, reason) => {
                    if let Some((id, Peer::Connected { .. })) = self.peers.lookup(&nid) {
                        if let Some((nid, _)) = self.disconnect(id, reason) {
                            self.metrics.peer(nid).disconnects += 1;
                        }
                    } else {
                        log::warn!(target: "wire", "Peer {nid} is not connected: ignoring disconnect");
                    }
                }
                Io::Wakeup(d) => {
                    self.actions.push_back(reactor::Action::SetTimer(d.into()));
                }
                Io::Fetch {
                    rid,
                    remote,
                    timeout,
                    reader_limit,
                    refs_at,
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
                    let (stream, channels) =
                        streams.open(ChannelsConfig::new(timeout).with_reader_limit(reader_limit));

                    log::debug!(target: "wire", "Opened new stream with id {stream} for {rid} and remote {remote}");

                    let link = *link;
                    let task = Task {
                        fetch: FetchRequest::Initiator {
                            rid,
                            remote,
                            refs_at,
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
                    if let Err(e) = self.worker.try_send(task) {
                        log::error!(
                            target: "wire",
                            "Worker pool failed to accept outgoing fetch request: {e}"
                        );
                    }
                    let metrics = self.metrics.peer(remote);
                    metrics.streams_opened += 1;
                    metrics.sent_fetch_requests += 1;

                    self.actions.push_back(Action::Send(
                        fd,
                        Frame::<service::Message>::control(link, frame::Control::Open { stream })
                            .encode_to_vec(),
                    ));
                }
            }
        }
        self.actions.pop_front()
    }
}

/// Establish a new outgoing connection.
pub fn dial<G: Ecdh<Pk = NodeId>>(
    remote_addr: NetAddr<HostName>,
    remote_id: <G as EcSk>::Pk,
    signer: G,
    config: &radicle::node::Config,
) -> io::Result<WireSession<G>> {
    // Determine what address to establish a TCP connection with, given the remote peer
    // address and our node configuration.
    let inet_addr: NetAddr<InetHost> = match (&remote_addr.host, config.proxy) {
        // For IP and DNS addresses, use the global proxy if set, otherwise use the address as-is.
        (HostName::Ip(_), Some(proxy)) => proxy.into(),
        (HostName::Ip(ip), None) => NetAddr::new(InetHost::Ip(*ip), remote_addr.port),
        (HostName::Dns(_), Some(proxy)) => proxy.into(),
        (HostName::Dns(dns), None) => NetAddr::new(InetHost::Dns(dns.clone()), remote_addr.port),
        // For onion addresses, handle with care.
        (HostName::Tor(onion), proxy) => match config.onion {
            // In onion proxy mode, simply use the configured proxy address.
            // This takes precedence over any global proxy.
            Some(AddressConfig::Proxy { address }) => address.into(),
            // In "forward" mode, if a global proxy is set, we use that, otherwise
            // we treat `.onion` addresses as regular DNS names.
            Some(AddressConfig::Forward) => {
                if let Some(proxy) = proxy {
                    proxy.into()
                } else {
                    NetAddr::new(InetHost::Dns(onion.to_string()), remote_addr.port)
                }
            }
            // If onion address support isn't configured, refuse to connect.
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "no configuration found for .onion addresses",
                ));
            }
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "unsupported remote address type",
            ));
        }
    };

    let addr = {
        use std::net::ToSocketAddrs as _;

        inet_addr
            .to_socket_addrs()?
            .next()
            .ok_or(io::ErrorKind::AddrNotAvailable)?
    };

    // NOTE: Previously, here was a note about setting the timeout for connecting
    // to DEFAULT_DIAL_TIMEOUT, for which we have not figured out a way yet.
    // Generally, we should understand what happens if the following call to
    // `connect` fails. How do we learn about it? Where's the leak?

    let connection = TcpStream::connect(addr)?;

    // Whether to tunnel regular connections through the proxy.
    let force_proxy = config.proxy.is_some();

    Ok(session::<G>(
        remote_addr,
        Some(remote_id),
        connection,
        force_proxy,
        signer,
    ))
}

/// Accept a new connection.
pub fn accept<G: Ecdh<Pk = NodeId>>(
    remote_addr: NetAddr<HostName>,
    connection: TcpStream,
    signer: G,
) -> WireSession<G> {
    session::<G>(remote_addr, None, connection, false, signer)
}

/// Create a new [`WireSession`].
fn session<G: Ecdh<Pk = NodeId>>(
    remote_addr: NetAddr<HostName>,
    remote_id: Option<NodeId>,
    connection: TcpStream,
    force_proxy: bool,
    signer: G,
) -> WireSession<G> {
    if let Err(e) = connection.set_nodelay(true) {
        log::warn!(target: "wire", "Unable to set TCP_NODELAY on socket {connection:?}: {e}");
    }

    let connection = std::net::TcpStream::from(connection);

    if let Err(e) = connection.set_read_timeout(Some(DEFAULT_CONNECTION_TIMEOUT)) {
        log::warn!(target: "wire", "Unable to set TCP read timeout on socket {connection:?}: {e}");
    }

    if let Err(e) = connection.set_write_timeout(Some(DEFAULT_CONNECTION_TIMEOUT)) {
        log::warn!(target: "wire", "Unable to set TCP write timeout on socket {connection:?}: {e}");
    }

    #[cfg(feature = "socket2")]
    {
        let connection = socket2::SockRef::from(&connection);

        let ka = socket2::TcpKeepalive::new()
            .with_time(time::Duration::from_secs(30))
            .with_interval(time::Duration::from_secs(10));

        #[cfg(not(windows))]
        let ka = ka.with_retries(3);

        if let Err(e) = connection.set_tcp_keepalive(&ka) {
            log::warn!(target: "wire", "Failed to set TCP_KEEPALIVE on socket {connection:?}: {e}");
        }
    }

    #[cfg(not(feature = "socket2"))]
    log::debug!(target: "wire", "Not attempting to set TCP_KEEPALIVE on socket {connection:?}");

    let connection = TcpStream::from_std(connection);

    let proxy = {
        let socks5 = socks5::Socks5::with(remote_addr, force_proxy);
        Socks5Session::new(connection, socks5)
    };

    let noise = {
        let pair = G::generate_keypair();

        let keyset = Keyset {
            e: pair.0,
            s: Some(signer),
            re: None,
            rs: remote_id,
        };

        NoiseState::initialize::<{ Sha256::OUTPUT_LEN }>(NOISE_XK, remote_id.is_some(), &[], keyset)
    };

    WireSession::new(proxy, noise)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::service::{Message, ZeroBytes};
    use crate::wire;
    use crate::wire::varint;

    #[test]
    fn test_pong_message_with_extension() {
        use radicle_protocol::deserializer;

        let mut stream = Vec::new();
        let pong = Message::Pong {
            zeroes: ZeroBytes::new(42),
        };
        frame::PROTOCOL_VERSION_STRING.encode(&mut stream);
        frame::StreamId::gossip(Link::Outbound).encode(&mut stream);

        // Serialize gossip message with some extension fields.
        let mut gossip = pong.encode_to_vec();
        String::from("extra").encode(&mut gossip);
        48u8.encode(&mut gossip);

        // Encode gossip message using the varint-prefix format into the stream.
        varint::payload::encode(&gossip, &mut stream);

        let mut de = deserializer::Deserializer::<1024, Frame>::new(1024);
        de.input(&stream).unwrap();

        // The "pong" message decodes successfully, even though there is trailing data.
        assert_eq!(
            de.deserialize_next().unwrap().unwrap(),
            Frame::gossip(Link::Outbound, pong)
        );
        assert!(de.deserialize_next().unwrap().is_none());
        assert!(de.is_empty());
    }

    #[test]
    fn test_inventory_ann_with_extension() {
        use radicle_protocol::deserializer;

        #[derive(Debug)]
        struct MessageWithExt {
            msg: Message,
            ext: String,
        }

        impl wire::Encode for MessageWithExt {
            fn encode(&self, writer: &mut impl bytes::BufMut) {
                self.msg.encode(writer);
                self.ext.encode(writer);
            }
        }

        impl wire::Decode for MessageWithExt {
            fn decode(reader: &mut impl bytes::Buf) -> Result<Self, wire::Error> {
                let msg = Message::decode(reader)?;
                let ext = String::decode(reader).unwrap_or_default();

                Ok(MessageWithExt { msg, ext })
            }
        }

        let rid = radicle::test::arbitrary::gen(1);
        let pk = radicle::test::arbitrary::gen(1);
        let sig: [u8; 64] = radicle::test::arbitrary::gen(1);

        // Message with extension.
        let mut stream = Vec::new();
        let ann = Message::announcement(
            pk,
            service::gossip::inventory(radicle::node::Timestamp::MAX, [rid]),
            radicle::crypto::Signature::from(sig),
        );
        let pong = Message::Pong {
            zeroes: ZeroBytes::new(42),
        };
        // Framed message with extension.
        frame::Frame::gossip(
            Link::Outbound,
            MessageWithExt {
                msg: ann.clone(),
                ext: String::from("extra"),
            },
        )
        .encode(&mut stream);
        // Pong message that comes after, without extension.
        frame::Frame::gossip(Link::Outbound, pong.clone()).encode(&mut stream);

        // First test deserializing using the message with extension type.
        {
            let mut de = deserializer::Deserializer::<1024, Frame<MessageWithExt>>::new(1024);
            de.input(&stream).unwrap();

            radicle::assert_matches!(
                de.deserialize_next().unwrap().unwrap().data,
                FrameData::Gossip(MessageWithExt {
                    msg,
                    ext,
                }) if msg == ann && ext == *"extra"
            );
            radicle::assert_matches!(
                de.deserialize_next().unwrap().unwrap().data,
                FrameData::Gossip(MessageWithExt {
                    msg,
                    ext,
                }) if msg == pong && ext.is_empty()
            );
            assert!(de.deserialize_next().unwrap().is_none());
            assert!(de.is_empty());
        }

        // Then test deserializing using the current message type without the extension.
        {
            let mut de = deserializer::Deserializer::<1024, Frame<Message>>::new(1024);
            de.input(&stream).unwrap();

            radicle::assert_matches!(
                de.deserialize_next().unwrap().unwrap().data,
                FrameData::Gossip(msg)
                if msg == ann
            );
            radicle::assert_matches!(
                de.deserialize_next().unwrap().unwrap().data,
                FrameData::Gossip(msg)
                if msg == pong
            );
            assert!(de.deserialize_next().unwrap().is_none());
            assert!(de.is_empty());
        }
    }
}
