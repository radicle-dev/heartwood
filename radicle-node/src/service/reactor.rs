use std::collections::VecDeque;

use log::*;

use crate::prelude::*;
use crate::service::session::Session;
use crate::storage::{FetchError, Namespaces, RefUpdate};

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
    pub repo: Id,
    /// Namespaces to fetch.
    pub namespaces: Namespaces,
    /// Remote peer we are interacting with.
    pub remote: NodeId,
    /// Indicates whether the fetch request was initiated by us.
    pub initiated: bool,
}

/// Result of a fetch request from a specific seed.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum FetchResult {
    /// Successful fetch from a seed.
    Fetched { updated: Vec<RefUpdate> },
    /// Error fetching the resource from a seed.
    Error { from: NodeId, error: FetchError },
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
        // TODO: Make sure we don't try to connect more than once to the same address.
        self.io.push_back(Io::Connect(id, addr));
    }

    /// Disconnect a peer.
    pub fn disconnect(&mut self, id: NodeId, reason: DisconnectReason) {
        self.io.push_back(Io::Disconnect(id, reason));
    }

    pub fn write(&mut self, remote: NodeId, msg: Message) {
        debug!("Write {:?} to {}", &msg, remote);

        self.io.push_back(Io::Write(remote, vec![msg]));
    }

    pub fn write_all(&mut self, remote: NodeId, msgs: impl IntoIterator<Item = Message>) {
        self.io
            .push_back(Io::Write(remote, msgs.into_iter().collect()));
    }

    pub fn wakeup(&mut self, after: LocalDuration) {
        self.io.push_back(Io::Wakeup(after));
    }

    pub fn fetch(&mut self, remote: NodeId, repo: Id, namespaces: Namespaces, initiated: bool) {
        if initiated {
            debug!("Fetch initiated for {} from {}..", repo, remote);
        } else {
            debug!("Fetch requested for {} from {}..", repo, remote);
        }
        self.io.push_back(Io::Fetch(Fetch {
            repo,
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
