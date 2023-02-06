use std::collections::VecDeque;

use log::*;

use crate::prelude::*;
use crate::service::session::Session;
use crate::storage::Namespaces;

use super::message::{Announcement, AnnouncementMessage};

/// Output of a state transition.
#[derive(Debug)]
pub enum Io {
    /// There are some messages ready to be sent to a peer.
    Write(NodeId, Vec<Message>),
    /// Connect to a peer.
    Connect(NodeId, Address),
    /// Disconnect from a peer.
    Disconnect(NodeId, DisconnectReason),
    /// Fetch repository data from a peer.
    Fetch(Fetch),
    /// Ask for a wakeup in a specified amount of time.
    Wakeup(LocalDuration),
    /// Emit an event.
    Event(Event),
}

/// Fetch job sent to worker thread.
#[derive(Debug, Clone)]
pub struct Fetch {
    /// Repo to fetch.
    pub rid: Id,
    /// Namespaces to fetch.
    pub namespaces: Namespaces,
    /// Remote peer we are interacting with.
    pub remote: NodeId,
    /// Indicates whether the fetch request was initiated by us.
    pub initiated: bool,
}

/// Interface to the network reactor.
#[derive(Debug, Default)]
pub struct Reactor {
    /// Outgoing I/O queue.
    io: VecDeque<Io>,
}

impl Reactor {
    /// Emit an event.
    pub fn event(&mut self, event: Event) {
        self.io.push_back(Io::Event(event));
    }

    /// Connect to a peer.
    pub fn connect(&mut self, id: NodeId, addr: Address) {
        self.io.push_back(Io::Connect(id, addr));
    }

    /// Disconnect a peer.
    pub fn disconnect(&mut self, id: NodeId, reason: DisconnectReason) {
        self.io.push_back(Io::Disconnect(id, reason));
    }

    pub fn write(&mut self, remote: NodeId, msg: Message) {
        debug!(target: "service", "Write {:?} to {}", &msg, remote);

        self.io.push_back(Io::Write(remote, vec![msg]));
    }

    pub fn write_all(&mut self, remote: NodeId, msgs: impl IntoIterator<Item = Message>) {
        let msgs = msgs.into_iter().collect::<Vec<_>>();
        for (ix, msg) in msgs.iter().enumerate() {
            debug!(
                target: "service",
                "Write {:?} message to {} ({}/{})",
                msg,
                remote,
                ix + 1,
                msgs.len()
            );
        }
        self.io.push_back(Io::Write(remote, msgs));
    }

    pub fn wakeup(&mut self, after: LocalDuration) {
        self.io.push_back(Io::Wakeup(after));
    }

    pub fn fetch(&mut self, remote: NodeId, rid: Id, namespaces: Namespaces, initiated: bool) {
        self.io.push_back(Io::Fetch(Fetch {
            rid,
            namespaces,
            remote,
            initiated,
        }));
    }

    /// Broadcast a message to a list of peers.
    pub fn broadcast<'a>(
        &mut self,
        msg: Announcement,
        peers: impl IntoIterator<Item = &'a Session>,
    ) {
        for peer in peers {
            self.write(peer.id, msg.clone().into());
        }
    }

    /// Relay a message to interested peers.
    pub fn relay<'a>(&mut self, ann: Announcement, peers: impl IntoIterator<Item = &'a Session>) {
        if let AnnouncementMessage::Refs(msg) = &ann.message {
            let id = msg.id;
            let peers = peers.into_iter().filter(|p| {
                if let Some(subscribe) = &p.subscribe {
                    subscribe.filter.contains(&id)
                } else {
                    // If the peer did not send us a `subscribe` message, we don'the
                    // relay any messages to them.
                    false
                }
            });
            self.broadcast(ann, peers);
        } else {
            self.broadcast(ann, peers);
        }
    }

    #[cfg(any(test, feature = "test"))]
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
