use std::collections::VecDeque;
use std::net;

use log::*;

use crate::prelude::*;
use crate::service::peer::Session;

/// Output of a state transition.
#[derive(Debug)]
pub enum Io {
    /// There are some messages ready to be sent to a peer.
    Write(net::SocketAddr, Vec<Envelope>),
    /// Connect to a peer.
    Connect(net::SocketAddr),
    /// Disconnect from a peer.
    Disconnect(net::SocketAddr, DisconnectReason),
    /// Ask for a wakeup in a specified amount of time.
    Wakeup(LocalDuration),
    /// Emit an event.
    Event(Event),
}

/// Interface to the network reactor.
#[derive(Debug)]
pub struct Reactor {
    /// The network we're on.
    network: Network,
    /// Outgoing I/O queue.
    io: VecDeque<Io>,
}

impl Reactor {
    pub fn new(network: Network) -> Self {
        Self {
            network,
            io: VecDeque::new(),
        }
    }

    /// Emit an event.
    pub fn event(&mut self, event: Event) {
        self.io.push_back(Io::Event(event));
    }

    /// Connect to a peer.
    pub fn connect(&mut self, addr: net::SocketAddr) {
        // TODO: Make sure we don't try to connect more than once to the same address.
        self.io.push_back(Io::Connect(addr));
    }

    /// Disconnect a peer.
    pub fn disconnect(&mut self, addr: net::SocketAddr, reason: DisconnectReason) {
        self.io.push_back(Io::Disconnect(addr, reason));
    }

    pub fn write(&mut self, remote: net::SocketAddr, msg: Message) {
        debug!("Write {:?} to {}", &msg, remote.ip());

        let envelope = self.network.envelope(msg);
        self.io.push_back(Io::Write(remote, vec![envelope]));
    }

    pub fn write_all(&mut self, remote: net::SocketAddr, msgs: impl IntoIterator<Item = Message>) {
        let envelopes = msgs
            .into_iter()
            .map(|msg| self.network.envelope(msg))
            .collect();
        self.io.push_back(Io::Write(remote, envelopes));
    }

    pub fn wakeup(&mut self, after: LocalDuration) {
        self.io.push_back(Io::Wakeup(after));
    }

    /// Broadcast a message to a list of peers.
    pub fn broadcast<'a>(&mut self, msg: Message, peers: impl IntoIterator<Item = &'a Session>) {
        for peer in peers {
            self.write(peer.addr, msg.clone());
        }
    }

    /// Relay a message to interested peers.
    pub fn relay<'a>(&mut self, msg: Message, peers: impl IntoIterator<Item = &'a Session>) {
        if let Message::RefsAnnouncement { message, .. } = &msg {
            let id = message.id.clone();
            let peers = peers.into_iter().filter(|p| {
                if let Some(subscribe) = &p.subscribe {
                    subscribe.filter.contains(&id)
                } else {
                    // If the peer did not send us a `subscribe` message, we don'the
                    // relay any messages to them.
                    false
                }
            });
            self.broadcast(msg, peers);
        } else {
            self.broadcast(msg, peers);
        }
    }

    #[cfg(test)]
    pub(crate) fn outbox(&mut self) -> &mut VecDeque<Io> {
        &mut self.io
    }
}

impl Iterator for Reactor {
    type Item = Io;

    fn next(&mut self) -> Option<Self::Item> {
        self.io.pop_front()
    }
}
