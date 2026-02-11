use std::collections::HashMap;

use radicle::storage::refs::RefsAt;
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
    subscribers: HashMap<FetchKey, Vec<(Vec<RefsAt>, S)>>,
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
        let refs_at = cmd.refs_at.clone();
        let event = self.state.fetch(cmd);

        let rejected = match &event {
            event::Fetch::QueueAtCapacity { .. } => subscriber,
            _ => {
                if let Some(r) = subscriber {
                    self.subscribers.entry(key).or_default().push((refs_at, r));
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

        let subscribers = if let event::Fetched::Completed {
            refs_at: ref fetched_refs,
            ..
        } = event
        {
            if let Some(subs) = self.subscribers.get_mut(&key) {
                // satisfied: subscribers whos 'wanted refs' match 'fetched_refs'
                // remaining: all other subscribers
                let (satisfied, remaining): (Vec<_>, Vec<_>) = subs
                    .drain(..)
                    .partition(|(wanted, _)| wanted == fetched_refs);

                // if everyone is satisfied, remove the entry; otherwise, put the remaining
                // ones back
                if remaining.is_empty() {
                    self.subscribers.remove(&key);
                } else {
                    *subs = remaining;
                }
                satisfied.into_iter().map(|(_, s)| s).collect()
            } else {
                Vec::new()
            }
        } else {
            // if the fetch failed or wasn't found, we notify everyone waiting on this key
            // because the connection/process failed entirely.
            self.subscribers
                .remove(&key)
                .unwrap_or_default()
                .into_iter()
                .map(|(_, s)| s)
                .collect()
        };

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
                orphaned.extend(subscribers.drain(..).map(|(_, r)| (key.rid, r)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetcher::MaxQueueSize;
    use radicle::test::arbitrary;
    use std::num::NonZeroUsize;
    use std::time::Duration;

    fn config(max_concurrency: usize, max_queue_size: usize) -> Config {
        Config::new()
            .with_max_concurrency(NonZeroUsize::new(max_concurrency).unwrap())
            .with_max_capacity(MaxQueueSize::new(
                NonZeroUsize::new(max_queue_size).unwrap(),
            ))
    }

    fn gen_refs_at(count: usize) -> Vec<RefsAt> {
        (0..count).map(|_| arbitrary::gen(1)).collect()
    }

    #[test]
    fn test_fetch_coalescing_different_refs() {
        let mut service = FetcherService::<usize>::new(config(1, 10));
        let node = arbitrary::gen(1);
        let repo = arbitrary::gen(1);
        let refs_specific = gen_refs_at(1);
        let refs_all = vec![];
        let timeout = Duration::from_secs(30);

        // fetch specific refs (Subscriber 1)
        let initiated1 = service.fetch(
            command::Fetch {
                from: node,
                rid: repo,
                refs_at: refs_specific.clone(),
                timeout,
            },
            Some(1),
        );
        assert!(matches!(initiated1.event, event::Fetch::Started { .. }));

        // fetch all refs (Subscriber 2)
        let initiated2 = service.fetch(
            command::Fetch {
                from: node,
                rid: repo,
                refs_at: refs_all.clone(),
                timeout,
            },
            Some(2),
        );

        // should be queued because refs differ
        assert!(matches!(initiated2.event, event::Fetch::Queued { .. }));

        // complete the specific refs fetch
        let completed = service.fetched(command::Fetched {
            from: node,
            rid: repo,
        });

        match completed.event {
            event::Fetched::Completed { ref refs_at, .. } => {
                assert_eq!(refs_at, &refs_specific);
            }
            _ => panic!("Expected Completed event"),
        }

        // only Subscriber 1 should be notified
        assert_eq!(completed.subscribers, vec![1]);

        // subscriber 2 should still be waiting
        assert!(service.subscribers.contains_key(&FetchKey::new(repo, node)));
        let remaining = &service.subscribers[&FetchKey::new(repo, node)];
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].1, 2);
    }
}
