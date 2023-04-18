use std::ops::Deref;
use std::time;

use crossbeam_channel as chan;

use crate::prelude::*;
use crate::storage::RefUpdate;

/// A service event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum Event {
    RefsFetched {
        remote: NodeId,
        rid: Id,
        updated: Vec<RefUpdate>,
    },
    RefsSynced {
        remote: NodeId,
        rid: Id,
    },
    SeedDiscovered {
        rid: Id,
        nid: NodeId,
    },
    SeedDropped {
        rid: Id,
        nid: NodeId,
    },
    PeerConnected {
        nid: NodeId,
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
    pub fn wait<F>(
        &self,
        mut f: F,
        timeout: time::Duration,
    ) -> Result<Event, chan::RecvTimeoutError>
    where
        F: FnMut(&Event) -> bool,
    {
        let start = time::Instant::now();

        loop {
            if let Some(timeout) = timeout.checked_sub(start.elapsed()) {
                match self.recv_timeout(timeout) {
                    Ok(event) => {
                        if f(&event) {
                            return Ok(event);
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
