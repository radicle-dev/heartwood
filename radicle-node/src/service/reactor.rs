use std::collections::{HashMap, VecDeque};
use std::mem;

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
}

/// Fetch job sent to worker thread.
#[derive(Debug, Clone)]
pub struct Fetch {
    /// Repo to fetch.
    pub rid: Id,
    /// Indicates whether the fetch request was initiated or is a response.
    pub direction: FetchDirection,
    /// Remote peer we are interacting with.
    pub remote: NodeId,
}

impl Fetch {
    pub fn is_initiator(&self) -> bool {
        self.direction.is_initiator()
    }

    pub fn initiated(&self) -> Option<&Namespaces> {
        match &self.direction {
            FetchDirection::Initiator { namespaces } => Some(namespaces),
            FetchDirection::Responder => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum FetchDirection {
    /// Client is initiating a fetch in order to receive the specified
    /// `refspecs` determined by [`Namespaces`].
    Initiator {
        /// Namespaces to fetch.
        namespaces: Namespaces,
    },
    /// Server is responding to a fetch request by uploading the
    /// specified `refspecs` sent by the client.
    Responder,
}

impl FetchDirection {
    pub fn is_initiator(&self) -> bool {
        matches!(self, Self::Initiator { .. })
    }
}

/// Interface to the network reactor.
#[derive(Debug, Default)]
pub struct Reactor {
    /// Outgoing I/O queue.
    io: VecDeque<Io>,
    /// Message outbox for each node.
    /// If messages can't be sent to a node immediately, they are stored in the outbox.
    /// This can happen if for eg. a fetch is ongoing with that node.
    outbox: HashMap<NodeId, Vec<Message>>,
}

impl Reactor {
    /// Connect to a peer.
    pub fn connect(&mut self, id: NodeId, addr: Address) {
        self.io.push_back(Io::Connect(id, addr));
    }

    /// Disconnect a peer.
    pub fn disconnect(&mut self, id: NodeId, reason: DisconnectReason) {
        self.io.push_back(Io::Disconnect(id, reason));
    }

    pub fn write(&mut self, remote: &Session, msg: Message) {
        // If we've requested a fetch or are currently fetching, any message to be written
        // to the remote peer should be queued.
        if remote.is_requesting() || remote.is_fetching() {
            debug!(target: "service", "Queue {:?} for {}", &msg, remote);
            self.outbox.entry(remote.id).or_default().push(msg);
        } else {
            debug!(target: "service", "Write {:?} to {}", &msg, remote);
            self.io.push_back(Io::Write(remote.id, vec![msg]));
        }
    }

    pub fn write_all(&mut self, remote: &Session, msgs: impl IntoIterator<Item = Message>) {
        let msgs = msgs.into_iter().collect::<Vec<_>>();
        let queue = remote.is_fetching() || remote.is_requesting();

        for (ix, msg) in msgs.iter().enumerate() {
            if queue {
                debug!(
                    target: "service",
                    "Queue {:?} for {} ({}/{})",
                    msg,
                    remote,
                    ix + 1,
                    msgs.len()
                );
            } else {
                debug!(
                    target: "service",
                    "Write {:?} to {} ({}/{})",
                    msg,
                    remote,
                    ix + 1,
                    msgs.len()
                );
            }
        }
        if queue {
            self.outbox.entry(remote.id).or_default().extend(msgs);
        } else {
            self.io.push_back(Io::Write(remote.id, msgs));
        }
    }

    pub fn drain(&mut self, remote: &Session) {
        if let Some(outbox) = self.outbox.get_mut(&remote.id) {
            debug!(target: "service", "Draining outbox for session {} ({} message(s))", remote.id, outbox.len());

            let msgs = mem::take(outbox);
            self.write_all(remote, msgs);
        }
    }

    pub fn wakeup(&mut self, after: LocalDuration) {
        self.io.push_back(Io::Wakeup(after));
    }

    pub fn fetch(&mut self, remote: &mut Session, rid: Id, direction: FetchDirection) {
        // Transition the session state machine to "fetching".
        remote.to_fetching(rid);

        self.io.push_back(Io::Fetch(Fetch {
            rid,
            direction,
            remote: remote.id,
        }));
    }

    /// Broadcast a message to a list of peers.
    pub fn broadcast<'a>(
        &mut self,
        msg: impl Into<Message>,
        peers: impl IntoIterator<Item = &'a Session>,
    ) {
        let msg = msg.into();
        for peer in peers {
            self.write(peer, msg.clone());
        }
    }

    /// Relay a message to interested peers.
    pub fn relay<'a>(&mut self, ann: Announcement, peers: impl IntoIterator<Item = &'a Session>) {
        if let AnnouncementMessage::Refs(msg) = &ann.message {
            let id = msg.rid;
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
