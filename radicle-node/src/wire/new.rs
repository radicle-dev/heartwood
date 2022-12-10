use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net;
use std::os::unix::io::AsRawFd;
use std::os::unix::prelude::RawFd;
use std::sync::Arc;
use std::time::Duration;

use cyphernet::addr::{HostAddr, NetAddr, PeerAddr};
use cyphernet::crypto::Ecdh;
use nakamoto_net::{DisconnectReason, Link, LocalTime};
use netservices::noise::NoiseXk;
use netservices::wire::{ListenerEvent, NetAccept, NetTransport, SessionEvent};
use netservices::{Frame, NetSession};
use radicle::collections::HashMap;
use radicle::crypto::Negotiator;
use radicle::node::NodeId;
use radicle::storage::WriteStorage;

use crate::crypto::Signer;
use crate::service::reactor::Io;
use crate::service::{routing, session, Message};
use crate::wire::{Decode, Encode};
use crate::{address, service, wire};

pub type Session<N: Negotiator> = NoiseXk<N>;

impl Frame for Message {
    type Error = wire::Error;

    fn unmarshall(mut reader: impl Read) -> Result<Option<Self>, Self::Error> {
        match Message::decode(&mut reader) {
            Ok(msg) => Ok(Some(msg)),
            Err(wire::Error::Io(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn marshall(&self, mut writer: impl Write) -> Result<usize, Self::Error> {
        self.encode(&mut writer).map_err(wire::Error::from)
    }
}

pub struct Wire<R, S, W, G, N: Negotiator> {
    inner: service::Service<R, S, W, G>,
    inner_queue:
        VecDeque<reactor::Action<NetAccept<Session<N>>, NetTransport<Session<N>, Message>>>,
    handshakes: HashMap<RawFd, Link>,
    sessions: HashMap<RawFd, NodeId>,
    // We use vec and not set since the same node may have multiple `N` sessions and has to
    // disconnect N-1 times (instead of disconnecting a single session)
    hangups: HashMap<RawFd, Option<DisconnectReason<service::DisconnectReason>>>,
    ecdh: N,
    proxy: net::SocketAddr,
}

impl<R, S, W, G, N> Wire<R, S, W, G, N>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer,
    N: Negotiator,
{
    pub fn new(
        mut inner: service::Service<R, S, W, G>,
        ecdh: N,
        proxy: net::SocketAddr,
        clock: LocalTime,
    ) -> Self {
        inner.initialize(clock);
        Self {
            inner,
            ecdh,
            proxy,
            inner_queue: empty!(),
            handshakes: empty!(),
            sessions: empty!(),
            hangups: empty!(),
        }
    }

    fn disconnect(&mut self, fd: RawFd, reason: DisconnectReason<service::DisconnectReason>) {
        let node_id = self
            .sessions
            .remove(&fd)
            .expect("disconnecting unknown peer");
        log::debug!("Attempting to disconnect {} due to {}", node_id, reason);
        self.hangups.insert(fd, Some(reason));
        self.inner_queue
            .push_back(reactor::Action::UnregisterTransport(fd));
    }
}

impl<R, S, W, G, N> reactor::Handler for Wire<R, S, W, G, N>
where
    R: routing::Store + Send,
    S: address::Store + Send,
    W: WriteStorage + Send + 'static,
    G: Signer + Send,
    N: Negotiator + Send,
    <N as Ecdh>::Pk: Send,
{
    type Listener = NetAccept<Session<N>>;
    type Transport = NetTransport<Session<N>, Message>;
    type Command = service::Command;

    fn handle_wakeup(&mut self) {
        self.inner.wake()
    }

    fn handle_listener_event(
        &mut self,
        socket_addr: net::SocketAddr,
        event: ListenerEvent<Session<N>>,
        duration: Duration,
    ) {
        self.inner.tick(LocalTime::from_secs(duration.as_secs()));

        match event {
            ListenerEvent::Accepted(session) => {
                log::debug!(
                    "Incoming connection from remote peer {}",
                    session.transition_addr()
                );
                self.handshakes.insert(session.as_raw_fd(), Link::Inbound);
                let transport = NetTransport::<Session<N>, Message>::upgrade(session)
                    .expect("socket failed configuration");
                self.inner.attempted(&socket_addr);
                self.inner_queue
                    .push_back(reactor::Action::RegisterTransport(transport))
            }
            ListenerEvent::Error(err) => {
                panic!("I/O error on the listener socket, {:?}", err)
            }
        }
        // TODO: Ensure we do not need to generate any events or do calls to the inner
    }

    fn handle_transport_event(
        &mut self,
        fd: RawFd,
        event: SessionEvent<Session<N>, Message>,
        duration: Duration,
    ) {
        self.inner.tick(LocalTime::from_secs(duration.as_secs()));

        match event {
            SessionEvent::SessionEstablished(node_id) => {
                if let Some((fd, _)) = self.sessions.iter().find(|(_, id)| **id == node_id) {
                    log::warn!(
                        "New session from the same peer {}, closing previous session",
                        node_id
                    );
                    self.disconnect(
                        *fd,
                        DisconnectReason::Protocol(service::DisconnectReason::RepeatedConnection),
                    );
                    return;
                }

                let link = *self
                    .handshakes
                    .get(&fd)
                    .expect("handshake completed for an unregistered peer");
                log::info!("New session established with {}", node_id);
                self.sessions
                    .insert(fd, node_id)
                    .expect("session file descriptor registered for the second time");
                self.inner.connected(node_id, link);
            }
            SessionEvent::Message(msg) => {
                let node_id = *self.sessions.get(&fd).expect("unknown session");
                log::debug!("Message {:?} from {}", msg, node_id);
                self.inner.received_message(node_id, msg);
            }
            SessionEvent::FrameFailure(err) => {
                self.disconnect(
                    fd,
                    DisconnectReason::Protocol(service::DisconnectReason::Error(
                        session::Error::Misbehavior,
                    )),
                );
            }
            SessionEvent::ConnectionFailure(err) => {
                self.disconnect(fd, DisconnectReason::ConnectionError(Arc::new(err)));
            }
            SessionEvent::Disconnected => {
                self.disconnect(
                    fd,
                    DisconnectReason::Protocol(service::DisconnectReason::Peer),
                );
            }
        }
    }

    fn handle_command(&mut self, cmd: Self::Command) {
        self.inner.command(cmd)
    }

    fn handle_error(&mut self, err: reactor::Error<net::SocketAddr, RawFd>) {
        match err {
            reactor::Error::ListenerUnknown(id) => {
                log::error!("asking for unknown listener {}", id);
            }
            reactor::Error::PeerUnknown(id) => {
                log::error!("asking for unknown peer {}", id);
            }
            reactor::Error::PeerDisconnected(addr, err) => {
                log::error!("the peer {} got disconnected with {}", addr, err);
                self.inner_queue
                    .push_back(reactor::Action::UnregisterTransport(addr))
            }
        }
    }

    fn handover_listener(&mut self, listener: Self::Listener) {
        unreachable!("for now we do not disconnect socket we listening on")
    }

    fn handover_transport(&mut self, transport: Self::Transport) {
        let fd = transport.as_raw_fd();
        if let Some(reason) = self.hangups.get(&fd) {
            if let Some(reason) = reason {
                self.inner.disconnected(&transport.expect_peer_id(), reason);
            }
        } else {
            todo!("send transport to the worker")
        }
    }
}

impl<R, S, W, G, N> Iterator for Wire<R, S, W, G, N>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer,
    N: Negotiator,
{
    type Item = reactor::Action<NetAccept<Session<N>>, NetTransport<Session<N>, Message>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(event) = self.inner_queue.pop_front() {
            return Some(event);
        }

        while let Some(ev) = self.inner.next() {
            match ev {
                Io::Write(node_id, msgs) => {
                    log::debug!("Sending {} messages to {}", msgs.len(), node_id);
                    let (fd, _) = self
                        .sessions
                        .iter()
                        .find(|(_, id)| **id == node_id)
                        .expect("sending message to the peer which is not connected");
                    return Some(reactor::Action::Send(*fd, msgs));
                }
                Io::Event(e) => {
                    log::warn!(
                        "Received an event, while events are not handled in the current version"
                    );
                    // TODO: Handle the events
                }
                Io::Connect(node_id, addr) => {
                    let NetAddr { host, port } = *addr;
                    let socket_addr = match host {
                        HostAddr::Ip(ip) => net::SocketAddr::new(ip, addr.port()),
                        HostAddr::Dns(_) => todo!(),
                        _ => self.proxy,
                    };

                    if self.sessions.values().any(|id| *id == node_id) {
                        log::error!("Attempt to connect already connected {}", node_id);
                        break;
                    }

                    match NetTransport::<Session<N>, Message>::connect(
                        PeerAddr::new(node_id, socket_addr),
                        &self.ecdh,
                    ) {
                        Ok(transport) => {
                            self.inner.attempted(&socket_addr);
                            self.handshakes
                                .insert(transport.as_raw_fd(), Link::Outbound);
                            return Some(reactor::Action::RegisterTransport(transport));
                        }
                        Err(err) => {
                            self.inner.disconnected(
                                &node_id,
                                &DisconnectReason::DialError(Arc::new(err)),
                            );
                            break;
                        }
                    }
                }
                Io::Disconnect(node_id, r) => {
                    let (fd, _) = self
                        .sessions
                        .iter()
                        .find(|(_, id)| **id == node_id)
                        .expect("service requested to disconnect unknown peer");
                    self.sessions.remove(fd);
                    self.hangups.insert(*fd, None);
                    return Some(reactor::Action::UnregisterTransport(*fd));
                }

                Io::Wakeup(d) => return Some(reactor::Action::Wakeup(d.into())),
            }
        }
        None
    }
}
