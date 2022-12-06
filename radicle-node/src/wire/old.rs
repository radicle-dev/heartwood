use std::net;

use cyphernet::addr::{Addr as _, HostAddr};
use nakamoto_net as nakamoto;
use nakamoto_net::{Link, LocalTime};
use radicle::storage::WriteStorage;

use crate::crypto::Signer;
use crate::service::reactor::Io;
use crate::service::routing;
use crate::wire::{Encode, Inbox, Wire};
use crate::{address, service};

#[derive(Debug)]
pub struct Inbox {
    pub deserializer: Deserializer,
    pub addr: net::SocketAddr,
}

#[derive(Debug)]
pub struct Wire<R, S, W, G> {
    node_ids: HashMap<net::SocketAddr, NodeId>,
    inner_queue: VecDeque<nakamoto::Io<service::Event, service::DisconnectReason>>,
    inboxes: HashMap<NodeId, Inbox>,
    inner: service::Service<R, S, W, G>,
    rng: fastrand::Rng,
}

impl<R, S, W, G> Wire<R, S, W, G> {
    pub fn new(mut inner: service::Service<R, S, W, G>) -> Self {
        // TODO: inner.initialize(LocalTime::new());
        Self {
            node_ids: HashMap::new(),
            inner_queue: Default::default(),
            inboxes: HashMap::new(),
            inner,
            rng: fastrand::Rng::new(),
        }
    }
}

impl<R, S, W, G> nakamoto::Protocol for Wire<R, S, W, G>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer,
{
    type Event = service::Event;
    type Command = service::Command;
    type DisconnectReason = service::DisconnectReason;

    fn initialize(&mut self, time: LocalTime) {
        self.inner.initialize(time)
    }

    fn tick(&mut self, now: nakamoto::LocalTime) {
        self.inner.tick(now)
    }

    fn wake(&mut self) {
        self.inner.wake()
    }

    fn command(&mut self, cmd: Self::Command) {
        self.inner.command(cmd)
    }

    fn attempted(&mut self, addr: &std::net::SocketAddr) {
        self.inner.attempted(addr)
    }

    fn connected(&mut self, addr: net::SocketAddr, local_addr: &net::SocketAddr, link: Link) {
        self.inner.connecting(addr, local_addr, link)
    }

    fn disconnected(
        &mut self,
        addr: &net::SocketAddr,
        reason: nakamoto::DisconnectReason<service::DisconnectReason>,
    ) {
        let node_id = self.node_ids[addr];

        self.inboxes.remove(&node_id);
        self.inner.disconnected(&node_id, &reason)
    }

    fn received_bytes(&mut self, addr: &net::SocketAddr, raw_bytes: &[u8]) {
        if let Some(Inbox { deserializer, .. }) = self
            .node_ids
            .get(addr)
            .and_then(|id| self.inboxes.get_mut(id))
        {
            let node_id = self.node_ids[addr];
            deserializer.input(&raw_bytes);
            for message in deserializer {
                match message {
                    Ok(msg) => self.inner.received_message(node_id, msg),
                    Err(err) => {
                        // TODO: Disconnect peer.
                        log::error!("Invalid message received from {}: {}", addr, err);

                        return;
                    }
                }
            }
        } else {
            log::debug!("Received message from unknown peer {}", addr);
        }
    }
}

impl<R, S, W, G> Iterator for Wire<R, S, W, G> {
    type Item = nakamoto::Io<service::Event, service::DisconnectReason>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(event) = self.inner_queue.pop_front() {
            return Some(event);
        }

        match self.inner.next() {
            Some(Io::Write(id, msgs)) => {
                let mut buf = Vec::new();
                for msg in msgs {
                    log::debug!("Write {:?} to {}", &msg, id);

                    msg.encode(&mut buf)
                        .expect("writing to an in-memory buffer doesn't fail");
                }
                let Inbox { addr, .. } = self.inboxes.get(&id).expect(
                    "broken handshake implementation: data sent before handshake was complete",
                );
                Some(nakamoto::Io::Write(*addr, buf))
            }
            Some(Io::Event(e)) => Some(nakamoto::Io::Event(e)),
            Some(Io::Connect(_id, addr)) => match addr.host {
                HostAddr::Ip(ip) => Some(nakamoto::Io::Connect(net::SocketAddr::from((
                    ip,
                    addr.port(),
                )))),
                _ => todo!(),
            },
            Some(Io::Disconnect(id, r)) => self
                .inboxes
                .get(&id)
                .map(|i| nakamoto::Io::Disconnect(i.addr, r)),
            Some(Io::Wakeup(d)) => Some(nakamoto::Io::Wakeup(d)),

            None => None,
        }
    }
}
