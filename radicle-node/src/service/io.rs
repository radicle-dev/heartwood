use std::collections::VecDeque;
use std::time;

use log::*;
use radicle::storage::refs::RefsAt;

use crate::prelude::*;
use crate::service::session::Session;
use crate::service::Link;

use super::gossip;
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

    /// Announce something to a peer. This is meant for our own announcement messages.
    pub fn announce<'a>(
        &mut self,
        ann: Announcement,
        peers: impl Iterator<Item = &'a Session>,
        gossip: &mut impl gossip::Store,
    ) {
        // Store our announcement so that it can be retrieved from us later, just like
        // announcements we receive from peers.
        if let Err(e) = gossip.announced(&ann.node, &ann) {
            error!(target: "service", "Error updating our gossip store with announced message: {e}");
        }

        for peer in peers {
            if let AnnouncementMessage::Refs(refs) = &ann.message {
                if let Some(subscribe) = &peer.subscribe {
                    if subscribe.filter.contains(&refs.rid) {
                        self.write(peer, ann.clone().into());
                    } else {
                        debug!(
                            target: "service",
                            "Skipping refs announcement relay to {peer}: peer isn't subscribed to {}",
                            refs.rid
                        );
                    }
                } else {
                    debug!(
                        target: "service",
                        "Skipping refs announcement relay to {peer}: peer didn't send a subscription filter"
                    );
                }
            } else {
                self.write(peer, ann.clone().into());
            }
        }
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
            msg.log(log::Level::Trace, &remote.id, Link::Outbound);
        }
        self.io.push_back(Io::Write(remote.id, msgs));
    }

    pub fn wakeup(&mut self, after: LocalDuration) {
        self.io.push_back(Io::Wakeup(after));
    }

    pub fn fetch(
        &mut self,
        peer: &mut Session,
        rid: RepoId,
        refs_at: Vec<RefsAt>,
        timeout: time::Duration,
    ) {
        peer.fetching(rid);

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
            remote: peer.id,
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
                    // If the peer did not send us a `subscribe` message, we don't
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
