//! Events for `upload-pack` processes.
pub mod upload_pack;
pub use upload_pack::UploadPack;

use std::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;
use std::time;

use crossbeam_channel as chan;

use crate::git::Oid;
use crate::node;
use crate::prelude::*;
use crate::storage::{refs, RefUpdate};

/// Maximum unconsumed events allowed per subscription.
pub const MAX_PENDING_EVENTS: usize = 8192;

/// A service event.
///
/// The node emits events of this type to its control socket for other
/// programs to consume.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Event {
    /// The node has received changes to Git references in a
    /// repository stored on the node, from another node.
    RefsFetched {
        /// The node identifier of the other node.
        remote: NodeId,
        /// The identifier of the repository in question.
        rid: RepoId,
        /// The list of Git references that were updated.
        updated: Vec<RefUpdate>,
    },
    /// The node has sent its list of Git references to another node
    /// and the other has fetched the updated references.
    RefsSynced {
        /// The node identifier of the other node.
        remote: NodeId,
        /// The identifier of the repository in question.
        rid: RepoId,
        /// The `rad/sigrefs` reference that was fetched.
        at: Oid,
    },
    /// The node has discovered a repository on new node on the
    /// Radicle network.
    SeedDiscovered {
        /// The identifier of the repository in question.
        rid: RepoId,
        /// The node identifier of the other node.
        nid: NodeId,
    },
    /// The node has dropped a repository on a node from its list of
    /// known repositories and nodes.
    SeedDropped {
        /// The identifier of the repository in question.
        rid: RepoId,
        /// The node identifier of the other node.
        nid: NodeId,
    },
    /// The node has connected directly to another node.
    PeerConnected {
        /// The node identifier of the other node.
        nid: NodeId,
    },
    /// The node has terminated its direct connection to another node.
    PeerDisconnected {
        /// The node identifier of the other node.
        nid: NodeId,
        /// The reason why the connection was terminated.
        reason: String,
    },
    /// The local node has received changes to Git references from its
    /// local user. In other words, the local user has pushed to the
    /// node, updated COBs, or otherwise updated refs in their local node.
    LocalRefsAnnounced {
        /// The identifier of the repository in question.
        rid: RepoId,
        /// List of changed Git references for the repository.
        refs: refs::RefsAt,
        /// When were the new references received? In other words,
        /// when did the user run `git push`?
        timestamp: Timestamp,
    },
    /// The node has received a message with a list of repositories on
    /// another node on the network.
    InventoryAnnounced {
        /// The node identifier of the other node.
        nid: NodeId,
        /// List of repositories sent.
        inventory: Vec<RepoId>,
        /// When was the list sent?
        timestamp: Timestamp,
    },
    /// The node has received a message about new signed Git
    /// references ("sigrefs") for a repository on another node on the
    /// network.
    RefsAnnounced {
        /// The node identifier of the other node.
        nid: NodeId,
        /// The identifier of the repository in question.
        rid: RepoId,
        /// List of Git references for the repository.
        refs: Vec<refs::RefsAt>,
        /// When was the list sent?
        timestamp: Timestamp,
    },
    /// The node received a message about a new node on the network.
    NodeAnnounced {
        /// The node identifier of the other node.
        nid: NodeId,
        /// Alias for the other node.
        alias: Alias,
        /// When was the announcement sent?
        timestamp: Timestamp,
        /// What features did the node advertise to the other node.
        features: node::Features,
        /// What of its addresses did the node tell the other node about?
        addresses: Vec<node::Address>,
    },
    /// The node has uploaded a Git pack file to another node.
    UploadPack(upload_pack::UploadPack),
}

impl From<upload_pack::UploadPack> for Event {
    fn from(value: upload_pack::UploadPack) -> Self {
        Self::UploadPack(value)
    }
}

/// Events feed.
pub struct Events(chan::Receiver<Event>);

impl IntoIterator for Events {
    type Item = Event;
    type IntoIter = chan::IntoIter<Event>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<chan::Receiver<Event>> for Events {
    fn from(value: chan::Receiver<Event>) -> Self {
        Self(value)
    }
}

impl Deref for Events {
    type Target = chan::Receiver<Event>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Events {
    /// Listen for events, and wait for the given predicate to return something,
    /// or timeout if the specified amount of time has elapsed.
    pub fn wait<F, T>(&self, mut f: F, timeout: time::Duration) -> Result<T, chan::RecvTimeoutError>
    where
        F: FnMut(&Event) -> Option<T>,
    {
        let start = time::Instant::now();

        loop {
            if let Some(timeout) = timeout.checked_sub(start.elapsed()) {
                match self.recv_timeout(timeout) {
                    Ok(event) => {
                        if let Some(output) = f(&event) {
                            return Ok(output);
                        }
                    }
                    Err(err @ chan::RecvTimeoutError::Disconnected) => {
                        return Err(err);
                    }
                    Err(chan::RecvTimeoutError::Timeout) => {
                        // Keep trying until our timeout reaches zero.
                        continue;
                    }
                }
            } else {
                return Err(chan::RecvTimeoutError::Timeout);
            }
        }
    }
}

/// Publishes events to subscribers.
#[derive(Debug, Clone)]
pub struct Emitter<T> {
    subscribers: Arc<Mutex<Vec<chan::Sender<T>>>>,
}

impl<T> Default for Emitter<T> {
    fn default() -> Emitter<T> {
        Emitter {
            subscribers: Default::default(),
        }
    }
}

impl<T: Clone> Emitter<T> {
    /// Emit event to subscribers and drop those who can't receive it.
    /// Nb. subscribers are also dropped if their channel is full.
    pub fn emit(&self, event: T) {
        // SAFETY: We deliberately propagate panics from other threads holding the lock.
        #[allow(clippy::unwrap_used)]
        self.subscribers
            .lock()
            .unwrap()
            .retain(|s| s.try_send(event.clone()).is_ok());
    }

    /// Subscribe to events stream.
    pub fn subscribe(&self) -> chan::Receiver<T> {
        let (sender, receiver) = chan::bounded(MAX_PENDING_EVENTS);
        // SAFETY: We deliberately propagate panics from other threads holding the lock.
        #[allow(clippy::unwrap_used)]
        let mut subs = self.subscribers.lock().unwrap();
        subs.push(sender);

        receiver
    }

    /// Number of subscribers.
    pub fn subscriptions(&self) -> usize {
        // SAFETY: We deliberately propagate panics from other threads holding the lock.
        #[allow(clippy::unwrap_used)]
        self.subscribers.lock().unwrap().len()
    }

    /// Number of messages that have not yet been received.
    pub fn pending(&self) -> usize {
        // SAFETY: We deliberately propagate panics from other threads holding the lock.
        #[allow(clippy::unwrap_used)]
        self.subscribers
            .lock()
            .unwrap()
            .iter()
            .map(|ch| ch.len())
            .sum()
    }
}
