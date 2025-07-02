use std::collections::VecDeque;
use std::time;

use localtime::LocalDuration;
use log::*;
use radicle::identity::RepoId;
use radicle::node::config::FetchPackSizeLimit;
use radicle::node::Address;
use radicle::node::NodeId;
use radicle::storage::refs::RefsAt;

use crate::connections::session;
use crate::connections::session::Session;
use crate::service::message::Message;
use crate::service::DisconnectReason;
use crate::service::Link;

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
        rid: RepoId,
        /// Remote node being fetched from.
        remote: NodeId,
        /// If the node is fetching specific `rad/sigrefs`.
        refs_at: Option<Vec<RefsAt>>,
        /// Fetch timeout.
        timeout: time::Duration,
        /// Limit the number of bytes fetched.
        reader_limit: FetchPackSizeLimit,
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

    // TODO(finto): use a `ConnectedNode` token that is a smart constructed
    // `NodeId`. We can take that instead of `Session<session::Connected>`,
    // which can relax the borrow-checker.
    pub fn write(&mut self, remote: &Session<session::Connected>, msg: Message) {
        let level = match &msg {
            Message::Ping(_) | Message::Pong { .. } => log::Level::Trace,
            _ => log::Level::Debug,
        };
        msg.log(level, &remote.node(), Link::Outbound);
        trace!(target: "service", "Write {:?} to {}", &msg, remote);

        self.io.push_back(Io::Write(remote.node(), vec![msg]));
    }

    /// Announce something to a peer. This is meant for our own announcement messages.
    pub fn announce<'a>(
        &mut self,
        ann: Announcement,
        peers: impl Iterator<Item = &'a Session<session::Connected>>,
    ) {
        for peer in peers {
            if let AnnouncementMessage::Refs(refs) = &ann.message {
                if peer.is_subscribed_to(&refs.rid) {
                    self.write(peer, ann.clone().into());
                } else {
                    debug!(
                        target: "service",
                        "Skipping refs announcement relay to {peer}: peer isn't subscribed to {}",
                        refs.rid
                    );
                }
            } else {
                self.write(peer, ann.clone().into());
            }
        }
    }

    pub fn write_all(&mut self, remote: NodeId, msgs: impl IntoIterator<Item = Message>) {
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
            msg.log(log::Level::Trace, &remote, Link::Outbound);
        }
        self.io.push_back(Io::Write(remote, msgs));
    }

    pub fn wakeup(&mut self, after: LocalDuration) {
        self.io.push_back(Io::Wakeup(after));
    }

    // TODO(finto): use a `ConnectedNode` token that is a smart constructed
    // `NodeId`. We can take that instead of `Session<session::Connected>`,
    // which can relax the borrow-checker.
    pub fn fetch(
        &mut self,
        peer: &Session<session::Connected>,
        rid: RepoId,
        refs_at: Vec<RefsAt>,
        timeout: time::Duration,
        reader_limit: FetchPackSizeLimit,
    ) {
        let refs_at = (!refs_at.is_empty()).then_some(refs_at);

        if let Some(refs_at) = &refs_at {
            debug!(
                target: "service",
                "Fetch initiated for {rid} with {peer} ({} remote(s))..", refs_at.len()
            );
        } else {
            debug!(target: "service", "Fetch initiated for {rid} with {peer} (all remotes)..");
        }

        self.io.push_back(Io::Fetch {
            rid,
            refs_at,
            remote: peer.node(),
            timeout,
            reader_limit,
        });
    }

    /// Broadcast a message to a list of peers.
    pub fn broadcast<'a>(
        &mut self,
        msg: impl Into<Message>,
        peers: impl IntoIterator<Item = &'a Session<session::Connected>>,
    ) {
        let msg = msg.into();
        for peer in peers {
            self.write(peer, msg.clone());
        }
    }

    /// Relay a message to interested peers.
    pub fn relay<'a>(
        &mut self,
        ann: Announcement,
        peers: impl IntoIterator<Item = &'a Session<session::Connected>>,
    ) {
        if let AnnouncementMessage::Refs(msg) = &ann.message {
            let id = msg.rid;
            let peers = peers.into_iter().filter(|p| p.is_subscribed_to(&id));
            self.broadcast(ann, peers);
        } else {
            self.broadcast(ann, peers);
        }
    }

    /// Number of items in outbox.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.io.len()
    }

    #[cfg(any(test, feature = "test"))]
    pub fn queue(&mut self) -> &mut VecDeque<Io> {
        &mut self.io
    }
}

impl Iterator for Outbox {
    type Item = Io;

    fn next(&mut self) -> Option<Self::Item> {
        self.io.pop_front()
    }
}
