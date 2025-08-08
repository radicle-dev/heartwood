use std::collections::HashMap;

use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::{command, event, Config, FetcherState, QueuedFetch};

/// Service layer that wraps [`FetcherState`] and manages subscriber coalescing.
///
/// When multiple callers request the same fetch, their subscribers are collected
/// and all notified when the fetch completes.
///
/// # Type Parameter
/// - `S`: The subscriber type (e.g., `chan::Sender<FetchResult>`).
#[derive(Debug)]
pub struct FetcherService<S> {
    state: FetcherState,
    subscribers: HashMap<FetchKey, Vec<S>>,
}

impl<S> FetcherService<S> {
    /// Initialize the [`FetcherService`] with the give [`Config`].
    pub fn new(config: Config) -> Self {
        Self {
            state: FetcherState::new(config),
            subscribers: HashMap::new(),
        }
    }

    /// Provide a reference handle to the [`FetcherState`].
    pub fn state(&self) -> &FetcherState {
        &self.state
    }
}

/// Key for pending subscribers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct FetchKey {
    rid: RepoId,
    node: NodeId,
}

impl FetchKey {
    fn new(rid: RepoId, node: NodeId) -> Self {
        Self { rid, node }
    }
}

/// The result of calling [`FetcherService::fetch`].
#[must_use]
#[derive(Debug)]
pub struct FetchInitiated<S> {
    /// The underlying result from calling [`FetcherState::fetch`].
    pub event: event::Fetch,
    /// Subscriber returned if fetch was rejected (queue at capacity).
    pub rejected: Option<S>,
}

/// The result of calling [`FetcherService::fetched`].
#[must_use]
#[derive(Debug)]
pub struct FetchCompleted<S> {
    /// The underlying result from calling [`FetcherState::fetched`].
    pub event: event::Fetched,
    /// All the subscribers that were interested in this given fetch.
    pub subscribers: Vec<S>,
}

/// The result of calling [`FetcherService::cancel`].
#[must_use]
#[derive(Debug)]
pub struct FetchesCancelled<S> {
    /// The underlying result from calling [`FetcherState::cancel`].
    pub event: event::Cancel,
    /// Orphaned subscribers paired with their [`RepoId`].
    pub orphaned: Vec<(RepoId, S)>,
}

impl<S> FetcherService<S> {
    /// Initiate a fetch, optionally registering a subscriber.
    ///
    /// Subscribers are coalesced: if the same `(rid, node)` is already being
    /// fetched or queued, the subscriber joins the existing waiters.
    ///
    /// If the fetch could not be initiated, and also could not be queued, then
    /// subscriber is returned to notify of the rejection.
    ///
    /// See [`FetcherState::fetch`].
    pub fn fetch(&mut self, cmd: command::Fetch, subscriber: Option<S>) -> FetchInitiated<S> {
        let key = FetchKey::new(cmd.rid, cmd.from);
        let event = self.state.fetch(cmd);

        let rejected = match &event {
            event::Fetch::QueueAtCapacity { .. } => subscriber,
            _ => {
                if let Some(r) = subscriber {
                    self.subscribers.entry(key).or_default().push(r);
                }
                None
            }
        };

        FetchInitiated { event, rejected }
    }

    /// Mark a fetch as completed and retrieve waiting subscribers.
    ///
    /// See [`FetcherState::fetched`].
    pub fn fetched(&mut self, cmd: command::Fetched) -> FetchCompleted<S> {
        let key = FetchKey::new(cmd.rid, cmd.from);
        let event = self.state.fetched(cmd);
        let subscribers = self.subscribers.remove(&key).unwrap_or_default();
        FetchCompleted { event, subscribers }
    }

    /// Cancel all fetches for a disconnected peer, returning any orphaned
    /// subscribers.
    ///
    /// See [`FetcherState::cancel`].
    pub fn cancel(&mut self, cmd: command::Cancel) -> FetchesCancelled<S> {
        let from = cmd.from;
        let event = self.state.cancel(cmd);

        let mut orphaned = Vec::new();
        self.subscribers.retain(|key, subscribers| {
            if key.node == from {
                orphaned.extend(subscribers.drain(..).map(|r| (key.rid, r)));
                false
            } else {
                true
            }
        });

        FetchesCancelled { event, orphaned }
    }

    /// Dequeue the next fetch for a node.
    ///
    /// See [`FetcherState::dequeue`].
    pub fn dequeue(&mut self, from: &NodeId) -> Option<QueuedFetch> {
        self.state.dequeue(from)
    }
}
