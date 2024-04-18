use std::ops::Deref;
use std::time;

use crossbeam_channel as chan;

use crate::git::Oid;
use crate::node;
use crate::prelude::*;
use crate::storage::{refs, RefUpdate};

/// A service event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Event {
    RefsFetched {
        remote: NodeId,
        rid: RepoId,
        updated: Vec<RefUpdate>,
    },
    RefsSynced {
        remote: NodeId,
        rid: RepoId,
        at: Oid,
    },
    SeedDiscovered {
        rid: RepoId,
        nid: NodeId,
    },
    SeedDropped {
        rid: RepoId,
        nid: NodeId,
    },
    PeerConnected {
        nid: NodeId,
    },
    PeerDisconnected {
        nid: NodeId,
        reason: String,
    },
    LocalRefsAnnounced {
        rid: RepoId,
        refs: refs::RefsAt,
        timestamp: Timestamp,
    },
    InventoryAnnounced {
        nid: NodeId,
        inventory: Vec<RepoId>,
        timestamp: Timestamp,
    },
    RefsAnnounced {
        nid: NodeId,
        rid: RepoId,
        refs: Vec<refs::RefsAt>,
        timestamp: Timestamp,
    },
    NodeAnnounced {
        nid: NodeId,
        alias: Alias,
        timestamp: Timestamp,
        features: node::Features,
        addresses: Vec<node::Address>,
    },
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
