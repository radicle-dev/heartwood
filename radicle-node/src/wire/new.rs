use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net;
use std::sync::Arc;
use std::time::Duration;

use cyphernet::addr::{HostAddr, LocalNode, NetAddr, PeerAddr};
use nakamoto_net::{DisconnectReason, Link, LocalTime};
use netservices::noise::{NoiseXk, XkAddr};
use netservices::wire::{ListenerEvent, NetAccept, NetTransport, SessionEvent};
use netservices::Frame;
use radicle::crypto::Ed25519;
use radicle::node::NodeId;
use radicle::storage::WriteStorage;

use crate::crypto::Signer;
use crate::service::reactor::Io;
use crate::service::{routing, session, Message};
use crate::wire::{Decode, Encode, Error};
use crate::{address, service, wire};

pub type Session = NoiseXk<Ed25519>;

#[derive(Clone, Debug, Default)]
pub struct Framer {
    read_queue: VecDeque<u8>,
    write_queue: VecDeque<u8>,
}

impl Frame for Framer {
    type Message = Message;
    type Error = wire::Error;

    fn push(&mut self, msg: Message) {
        msg.encode(&mut self.write_queue)
            .expect("writing to an in-memory buffer doesn't fail");
    }

    fn pop(&mut self) -> Result<Option<Message>, Self::Error> {
        if self.read_queue.is_empty() {
            return Ok(None);
        }
        // If the message was not received in full we roll back
        let mut cursor = io::Cursor::new(self.read_queue.make_contiguous());
        Message::decode(&mut cursor)
            .map(|msg| {
                self.read_queue.drain(..cursor.position() as usize);
                Some(msg)
            })
            .or_else(|err| match err {
                Error::Io(_) => Ok(None),
                // We don't roll back here since on the failed message the connection must be closed
                err => Err(err),
            })
    }

    fn queue_len(&self) -> usize {
        self.write_queue.len()
    }
}

impl Read for Framer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.write_queue.read(buf)
    }
}

impl Write for Framer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.read_queue.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        // Do nothing
        Ok(())
    }
}

pub struct Wire<R, S, W, G> {
    inner: service::Service<R, S, W, G>,
    inner_queue: VecDeque<reactor::Action<NetAccept<Session>, NetTransport<Session, Framer>>>,
    local_node: LocalNode<Ed25519>,
    proxy: net::SocketAddr,
}

impl<R, S, W, G> Wire<R, S, W, G>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer,
{
    pub fn new(
        mut inner: service::Service<R, S, W, G>,
        local_node: LocalNode<Ed25519>,
        proxy: net::SocketAddr,
        clock: LocalTime,
    ) -> Self {
        inner.initialize(clock);
        Self {
            inner,
            local_node,
            proxy,
            inner_queue: empty!(),
        }
    }
}

impl<R, S, W, G> reactor::Handler for Wire<R, S, W, G>
where
    R: routing::Store + Send,
    S: address::Store + Send,
    W: WriteStorage + Send + 'static,
    G: Signer,
{
    type Listener = NetAccept<Session>;
    type Transport = NetTransport<Session, Framer>;
    type Command = service::Command;

    fn handle_wakeup(&mut self) {
        self.inner.wake()
    }

    fn handle_listener_event(
        &mut self,
        socket_addr: net::SocketAddr,
        event: ListenerEvent<Session>,
        duration: Duration,
    ) {
        self.inner.tick(LocalTime::from_secs(duration.as_secs()));

        match event {
            ListenerEvent::Accepted(session) => {
                let transport = NetTransport::<Session, Framer>::upgrade(session)
                    .expect("socket can't be configured");
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
        addr: XkAddr<NodeId, net::SocketAddr>,
        event: SessionEvent<Session, Framer>,
        duration: Duration,
    ) {
        self.inner.tick(LocalTime::from_secs(duration.as_secs()));

        match event {
            SessionEvent::SessionEstablished(peer_addr) => {
                let link = match addr {
                    XkAddr::Partial(_) => Link::Inbound,
                    XkAddr::Full(_) => Link::Outbound,
                };
                self.inner.connected(*peer_addr.id(), link);
            }
            SessionEvent::Message(msg) => {
                let peer_addr = addr.expect_peer_addr();
                self.inner.received_message(*peer_addr.id(), msg);
            }
            SessionEvent::FrameFailure(err) => {
                let peer_addr = addr.expect_peer_addr();
                self.inner.disconnected(
                    peer_addr.id(),
                    &DisconnectReason::Protocol(service::DisconnectReason::Error(
                        session::Error::Misbehavior,
                    )),
                );
            }
            SessionEvent::ConnectionFailure(err) => {
                let peer_addr = addr.expect_peer_addr();
                self.inner.disconnected(
                    peer_addr.id(),
                    &DisconnectReason::ConnectionError(Arc::new(err)),
                );
            }
            SessionEvent::Disconnected => {
                let peer_addr = addr.expect_peer_addr();
                self.inner.disconnected(
                    peer_addr.id(),
                    &DisconnectReason::Protocol(service::DisconnectReason::Peer),
                );
            }
        }
    }

    fn handle_command(&mut self, cmd: Self::Command) {
        self.inner.command(cmd)
    }

    fn handle_error(
        &mut self,
        err: reactor::Error<net::SocketAddr, XkAddr<NodeId, net::SocketAddr>>,
    ) {
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
}

impl<R, S, W, G> Iterator for Wire<R, S, W, G>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer,
{
    type Item = reactor::Action<NetAccept<Session>, NetTransport<Session, Framer>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(event) = self.inner_queue.pop_front() {
            return Some(event);
        }

        match self.inner.next() {
            Some(Io::Write(id, msgs)) => {
                log::debug!("Sending {} messages to {}", msgs.len(), id);
                Some(reactor::Action::Send(id, msgs))
            }
            Some(Io::Event(e)) => {
                log::warn!(
                    "Received an event, while events are not handled in the current version"
                );
                // TODO: Handle the events
                None
            }
            Some(Io::Connect(id, addr)) => {
                let NetAddr { host, port } = *addr;
                let socket_addr = match host {
                    HostAddr::Ip(ip) => net::SocketAddr::new(ip, addr.port()),
                    HostAddr::Dns(_) => todo!(),
                    _ => self.proxy,
                };

                match NetTransport::<Session, Framer>::connect(
                    PeerAddr::new(id, socket_addr),
                    &self.local_node,
                ) {
                    Ok(transport) => {
                        self.inner.attempted(&socket_addr);
                        Some(reactor::Action::RegisterTransport(transport))
                    }
                    Err(err) => {
                        self.inner
                            .disconnected(&id, &DisconnectReason::DialError(Arc::new(err)));
                        return None;
                    }
                }
            }
            Some(Io::Disconnect(id, r)) => Some(reactor::Action::UnregisterTransport(id)),

            Some(Io::Wakeup(d)) => Some(reactor::Action::Wakeup(d.into())),

            None => None,
        }
    }
}
