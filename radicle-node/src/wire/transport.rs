//! Implementation of the transport protocol.
//!
//! We use the Noise XK handshake pattern to establish an encrypted stream with a remote peer.
//! The handshake itself is implemented in the external [`netservices`] crate.
use std::collections::VecDeque;
use std::os::unix::io::AsRawFd;
use std::os::unix::prelude::RawFd;
use std::sync::Arc;
use std::time::Instant;
use std::{io, net};

use crossbeam_channel as chan;
use cyphernet::addr::{Addr as _, HostAddr, PeerAddr};
use nakamoto_net::{Link, LocalTime};
use netservices::noise::NoiseXk;
use netservices::wire::{ListenerEvent, NetAccept, NetTransport, SessionEvent};
use netservices::NetSession;

use radicle::collections::HashMap;
use radicle::crypto::Negotiator;
use radicle::node::NodeId;
use radicle::storage::WriteStorage;

use crate::crypto::Signer;
use crate::service::reactor::{Fetch, Io};
use crate::service::{routing, session, DisconnectReason, Message, Service};
use crate::wire::{Decode, Encode};
use crate::worker::{WorkerReq, WorkerResp};
use crate::{address, service};

/// Reactor action.
type Action<G> = reactor::Action<NetAccept<NoiseXk<G>>, NetTransport<NoiseXk<G>>>;

/// Peer connection state machine.
#[derive(Debug)]
enum Peer<G: Negotiator> {
    /// The initial state before handshake is completed.
    Connecting { link: Link },
    /// The state after handshake is completed.
    /// Peers in this state are handled by the underlying service.
    Connected { link: Link, id: NodeId },
    /// The state after a peer was disconnected, either during handshake,
    /// or once connected.
    Disconnected {
        id: NodeId,
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
    Upgraded {
        link: Link,
        id: NodeId,
        response: chan::Receiver<WorkerResp<G>>,
    },
}

impl<G: Negotiator> Peer<G> {
    /// Return a new connecting peer.
    fn connecting(link: Link) -> Self {
        Self::Connecting { link }
    }

    /// Switch to connected state.
    fn connected(&mut self, id: NodeId) {
        if let Self::Connecting { link } = self {
            *self = Self::Connected { link: *link, id };
        } else {
            panic!("Peer::connected: session for {} is already established", id);
        }
    }

    /// Switch to disconnected state.
    fn disconnected(&mut self, reason: DisconnectReason) {
        if let Self::Connected { id, .. } = self {
            *self = Self::Disconnected { id: *id, reason };
        } else {
            panic!("Peer::disconnected: session is not connected");
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
    fn upgraded(&mut self, listener: chan::Receiver<WorkerResp<G>>) -> Fetch {
        if let Self::Upgrading { fetch, id, link } = self {
            let fetch = fetch.clone();

            *self = Self::Upgraded {
                id: *id,
                link: *link,
                response: listener,
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

/// Transport protocol implementation for a set of peers.
pub struct Transport<R, S, W, G: Negotiator> {
    /// Backing service instance.
    service: Service<R, S, W, G>,
    /// Worker pool interface.
    worker: chan::Sender<WorkerReq<G>>,
    /// Used to performs X25519 key exchange.
    keypair: G,
    /// Internal queue of actions to send to the reactor.
    actions: VecDeque<Action<G>>,
    /// Peer sessions.
    peers: HashMap<RawFd, Peer<G>>,
    /// SOCKS5 proxy address.
    proxy: net::SocketAddr,
    /// Buffer for incoming peer data.
    read_queue: VecDeque<u8>,
}

impl<R, S, W, G> Transport<R, S, W, G>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer + Negotiator,
{
    pub fn new(
        mut service: Service<R, S, W, G>,
        worker: chan::Sender<WorkerReq<G>>,
        keypair: G,
        proxy: net::SocketAddr,
        clock: LocalTime,
    ) -> Self {
        service.initialize(clock);

        Self {
            service,
            worker,
            keypair,
            proxy,
            actions: VecDeque::new(),
            peers: HashMap::default(),
            read_queue: VecDeque::new(),
        }
    }

    fn by_id(&self, id: &NodeId) -> RawFd {
        self.connected()
            .find(|(_, i)| *i == id)
            .map(|(fd, _)| fd)
            .unwrap()
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
        let Some(peer) = self.peers.get_mut(&fd) else {
            log::error!(target: "transport", "Peer with fd {fd} was not found");
            return;
        };
        if let Peer::Disconnected { .. } = peer {
            log::error!(target: "transport", "Peer with fd {fd} is already disconnected");
            return;
        };
        log::debug!(target: "transport", "Disconnecting peer with fd {} ({})..", fd, reason);
        peer.disconnected(reason);

        self.actions.push_back(Action::UnregisterTransport(fd));
    }

    fn upgrade(&mut self, fd: RawFd, fetch: Fetch) {
        let Some(peer) = self.peers.get_mut(&fd) else {
            log::error!(target: "transport", "Peer with fd {fd} was not found");
            return;
        };
        if let Peer::Disconnected { .. } = peer {
            log::error!(target: "transport", "Peer with fd {fd} is already disconnected");
            return;
        };
        log::debug!(target: "transport", "Requesting transport handover from reactor for fd {fd}");
        peer.upgrading(fetch);

        self.actions.push_back(Action::UnregisterTransport(fd));
    }

    fn upgraded(&mut self, session: NetTransport<NoiseXk<G>>) {
        let fd = session.as_raw_fd();
        let Some(peer) = self.peers.get_mut(&fd) else {
            log::error!(target: "transport", "Peer with fd {fd} was not found");
            return;
        };
        let (send, recv) = chan::bounded::<WorkerResp<G>>(1);
        let fetch = peer.upgraded(recv);

        if self
            .worker
            .send(WorkerReq {
                fetch,
                session,
                drain: self.read_queue.drain(..).collect(),
                channel: send,
            })
            .is_err()
        {
            log::error!(target: "transport", "Worker pool is disconnected; cannot send fetch request");
        }
    }

    fn fetch_complete(&mut self, resp: WorkerResp<G>) {
        let session = resp.session;
        let fd = session.as_raw_fd();
        let Some(peer) = self.peers.get_mut(&fd) else {
            log::error!(target: "transport", "Peer with fd {fd} was not found");
            return;
        };
        if let Peer::Disconnected { .. } = peer {
            log::error!(target: "transport", "Peer with fd {fd} is already disconnected");
            return;
        };
        peer.downgrade();

        self.actions.push_back(Action::RegisterTransport(session));
        self.service.fetch_complete(resp.result);
    }
}

impl<R, S, W, G> reactor::Handler for Transport<R, S, W, G>
where
    R: routing::Store + Send,
    S: address::Store + Send,
    W: WriteStorage + Send + 'static,
    G: Signer + Negotiator + Send,
{
    type Listener = NetAccept<NoiseXk<G>>;
    type Transport = NetTransport<NoiseXk<G>>;
    type Command = service::Command;

    fn tick(&mut self, time: Instant) {
        // FIXME: Ensure that the time correctly converted.
        self.service
            .tick(LocalTime::from_secs(time.elapsed().as_secs()));

        let mut completed = Vec::new();
        for peer in self.peers.values() {
            if let Peer::Upgraded { response, .. } = peer {
                if let Ok(resp) = response.try_recv() {
                    completed.push(resp);
                }
            }
        }
        for resp in completed {
            self.fetch_complete(resp);
        }
    }

    fn handle_wakeup(&mut self) {
        self.service.wake()
    }

    fn handle_listener_event(
        &mut self,
        socket_addr: net::SocketAddr,
        event: ListenerEvent<NoiseXk<G>>,
        _: Instant,
    ) {
        match event {
            ListenerEvent::Accepted(session) => {
                log::debug!(
                    target: "transport",
                    "Accepted inbound peer connection from {}..",
                    session.transition_addr()
                );
                self.peers
                    .insert(session.as_raw_fd(), Peer::connecting(Link::Inbound));

                let transport = match NetTransport::<NoiseXk<G>>::accept(session) {
                    Ok(transport) => transport,
                    Err(err) => {
                        log::error!(target: "transport", "Failed to upgrade accepted peer socket: {err}");
                        return;
                    }
                };
                self.service.accepted(socket_addr);
                self.actions
                    .push_back(reactor::Action::RegisterTransport(transport))
            }
            ListenerEvent::Failure(err) => {
                log::error!(target: "transport", "Error listening for inbound connections: {err}");
            }
        }
    }

    fn handle_transport_event(&mut self, fd: RawFd, event: SessionEvent<NoiseXk<G>>, _: Instant) {
        match event {
            SessionEvent::Established(node_id) => {
                log::debug!(target: "transport", "Session established with {node_id}");

                let conflicting = self
                    .connected()
                    .filter(|(_, id)| **id == node_id)
                    .map(|(fd, _)| fd)
                    .collect::<Vec<_>>();

                for fd in conflicting {
                    log::warn!(
                        target: "transport", "Closing conflicting session with {node_id} (fd={fd})"
                    );
                    self.disconnect(
                        fd,
                        DisconnectReason::Dial(
                            io::Error::from(io::ErrorKind::AlreadyExists).into(),
                        ),
                    );
                }

                let Some(peer) = self.peers.get_mut(&fd) else {
                    log::error!(target: "transport", "Session not found for fd {fd}");
                    return;
                };
                let Peer::Connecting { link } = peer else {
                    log::error!(
                        target: "transport",
                        "Session for {node_id} was either not found, or in an invalid state"
                    );
                    return;
                };
                let link = *link;

                peer.connected(node_id);
                self.service.connected(node_id, link);
            }
            SessionEvent::Data(data) => {
                if let Some(Peer::Connected { link, id }) = self.peers.get(&fd) {
                    self.read_queue.extend(data);

                    loop {
                        match Message::decode(&mut self.read_queue) {
                            Ok(msg) => {
                                log::debug!(
                                    target: "transport", "Received message {:?} from {} ({:?})", msg, id, link
                                );
                                self.service.received_message(*id, msg)
                            }
                            Err(err) if err.is_eof() => {
                                // Buffer is empty, or message isn't complete.
                                break;
                            }
                            Err(err) => {
                                // TODO(cloudhead): Include error in reason.
                                log::error!(target: "transport", "Invalid message from {}: {err}", id);
                                self.disconnect(
                                    fd,
                                    DisconnectReason::Session(session::Error::Misbehavior),
                                );
                                break;
                            }
                        }
                    }
                } else {
                    log::warn!(target: "transport", "Dropping message from unconnected peer with fd {fd}");
                }
            }
            SessionEvent::Terminated(err) => {
                self.disconnect(fd, DisconnectReason::Connection(Arc::new(err)));
            }
        }
    }

    fn handle_command(&mut self, cmd: Self::Command) {
        self.service.command(cmd);
    }

    fn handle_error(&mut self, err: reactor::Error<net::SocketAddr, RawFd>) {
        match err {
            reactor::Error::ListenerUnknown(id) => {
                log::error!(target: "transport", "Received error: unknown listener {}", id);
            }
            reactor::Error::PeerUnknown(id) => {
                log::error!(target: "transport", "Received error: unknown peer {}", id);
            }
            reactor::Error::PeerDisconnected(fd, err) => {
                log::error!(target: "transport", "Received error: peer {} disconnected: {}", fd, err);

                self.actions.push_back(Action::UnregisterTransport(fd));
            }
            reactor::Error::Poll(err) => {
                log::error!(target: "transport", "Can't poll connections: {}", err);
            }
        }
    }

    fn handover_listener(&mut self, _listener: Self::Listener) {
        panic!("Transport::handover_listener: listener handover is not supported");
    }

    fn handover_transport(&mut self, transport: Self::Transport) {
        let fd = transport.as_raw_fd();

        match self.peers.get(&fd) {
            Some(Peer::Disconnected { id, reason }) => {
                // Disconnect TCP stream.
                drop(transport);

                self.service.disconnected(*id, reason);
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

impl<R, S, W, G> Iterator for Transport<R, S, W, G>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer + Negotiator,
{
    type Item = reactor::Action<NetAccept<NoiseXk<G>>, NetTransport<NoiseXk<G>>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(event) = self.actions.pop_front() {
            return Some(event);
        }

        while let Some(ev) = self.service.next() {
            match ev {
                Io::Write(node_id, msgs) => {
                    log::debug!(
                        target: "transport", "Sending {} message(s) to {}", msgs.len(), node_id
                    );
                    let fd = self.by_id(&node_id);
                    let mut data = Vec::new();
                    for msg in msgs {
                        msg.encode(&mut data).expect("in-memory writes never fail");
                    }
                    return Some(reactor::Action::Send(fd, data));
                }
                Io::Event(_e) => {
                    log::warn!(
                        target: "transport", "Events are not currently supported"
                    );
                }
                Io::Connect(node_id, addr) => {
                    let socket_addr = match addr.host {
                        HostAddr::Ip(ip) => net::SocketAddr::new(ip, addr.port()),
                        HostAddr::Dns(_) => todo!(),
                        _ => self.proxy,
                    };

                    if self.connected().any(|(_, id)| id == &node_id) {
                        log::error!(
                            target: "transport",
                            "Attempt to connect to already connected peer {node_id}"
                        );
                        break;
                    }

                    match NetTransport::<NoiseXk<G>>::connect(
                        PeerAddr::new(node_id, socket_addr),
                        &self.keypair,
                    ) {
                        Ok(transport) => {
                            self.service.attempted(node_id, &socket_addr.into());
                            self.peers
                                .insert(transport.as_raw_fd(), Peer::connecting(Link::Outbound));

                            return Some(reactor::Action::RegisterTransport(transport));
                        }
                        Err(err) => {
                            self.service
                                .disconnected(node_id, &DisconnectReason::Dial(Arc::new(err)));
                            break;
                        }
                    }
                }
                Io::Disconnect(node_id, reason) => {
                    let fd = self.by_id(&node_id);
                    self.disconnect(fd, reason);

                    return self.actions.pop_back();
                }
                Io::Wakeup(d) => return Some(reactor::Action::SetTimer(d.into())),
                Io::Fetch(fetch) => {
                    // TODO: Check that the node_id is connected, queue request otherwise.
                    let fd = self.by_id(&fetch.remote);
                    self.upgrade(fd, fetch);
                }
            }
        }
        None
    }
}
