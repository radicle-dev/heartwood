use std::collections::{HashMap, VecDeque};
use std::time;

use log::*;
use radicle::git;
use radicle::storage::refs::RefsAt;

use crate::prelude::*;
use crate::service::session::Session;
use crate::service::Link;
use crate::storage::Namespaces;

use super::message::{Announcement, AnnouncementMessage};

/// I/O operation to execute at the network/wire level.
#[derive(Debug)]
pub enum Io {
    /// There are some messages ready to be sent to a peer.
    Write(NodeId, Vec<Message>),
    /// Connect to a peer.
    Connect(NodeId, Address),
    /// Disconnect from a peer.
    Disconnect(NodeId, DisconnectReason),
    /// Fetch repository data from a peer.
    Fetch {
        /// Repo being fetched.
        rid: Id,
        /// Remote node being fetched from.
        remote: NodeId,
        /// Namespaces being fetched.
        namespaces: Namespaces,
        /// If a refs announcements was made.
        refs_at: Option<HashMap<NodeId, git::Oid>>,
        /// Fetch timeout.
        timeout: time::Duration,
    },
    /// Ask for a wakeup in a specified amount of time.
    Wakeup(LocalDuration),
}

/// Interface to the network.
#[derive(Debug, Default)]
pub struct Outbox {
    /// Outgoing I/O queue.
    io: VecDeque<Io>,
}

impl Outbox {
    /// Connect to a peer.
    pub fn connect(&mut self, id: NodeId, addr: Address) {
        self.io.push_back(Io::Connect(id, addr));
    }

    /// Disconnect a peer.
    pub fn disconnect(&mut self, id: NodeId, reason: DisconnectReason) {
        self.io.push_back(Io::Disconnect(id, reason));
    }

    pub fn write(&mut self, remote: &Session, msg: Message) {
        msg.log(log::Level::Debug, &remote.id, Link::Outbound);
        trace!(target: "service", "Write {:?} to {}", &msg, remote);

        self.io.push_back(Io::Write(remote.id, vec![msg]));
    }

    pub fn write_all(&mut self, remote: &Session, msgs: impl IntoIterator<Item = Message>) {
        let msgs = msgs.into_iter().collect::<Vec<_>>();

        for (ix, msg) in msgs.iter().enumerate() {
            trace!(
                target: "service",
                "Write {:?} to {} ({}/{})",
                msg,
                remote,
                ix + 1,
                msgs.len()
            );
            msg.log(log::Level::Debug, &remote.id, Link::Outbound);
        }
        self.io.push_back(Io::Write(remote.id, msgs));
    }

    pub fn wakeup(&mut self, after: LocalDuration) {
        self.io.push_back(Io::Wakeup(after));
    }

    pub fn fetch(
        &mut self,
        remote: &mut Session,
        rid: Id,
        namespaces: Namespaces,
        refs_at: Vec<RefsAt>,
        timeout: time::Duration,
    ) {
        let refs_at = {
            let refs = refs_at
                .into_iter()
                .map(|RefsAt { remote, at }| (remote, at))
                .collect::<HashMap<_, _>>();
            (!refs.is_empty()).then_some(refs)
        };
        self.io.push_back(Io::Fetch {
            rid,
            namespaces,
            refs_at,
            remote: remote.id,
            timeout,
        });
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
    pub(crate) fn queue(&mut self) -> &mut VecDeque<Io> {
        &mut self.io
    }
}

impl Iterator for Outbox {
    type Item = Io;

    fn next(&mut self) -> Option<Self::Item> {
        self.io.pop_front()
    }
}
