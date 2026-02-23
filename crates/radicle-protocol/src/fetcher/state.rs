//! Logical state for Git fetches happening in the node.
//!
//! See [`FetcherState`] for more information.
//!
//! See [`command`]'s for input into [`FetcherState`].
//! See [`event`]'s for output from [`FetcherState`].

pub mod command;
pub mod event;

pub use command::Command;
pub use event::Event;

use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroUsize;
use std::time;

use radicle_core::{NodeId, RepoId};

use crate::fetcher::RefsToFetch;

/// Default for the maximum items per fetch queue.
pub const MAX_FETCH_QUEUE_SIZE: usize = 128;
/// Default for maximum concurrency per node.
pub const MAX_CONCURRENCY: NonZeroUsize = NonZeroUsize::MIN;

/// Logical state for Git fetches happening in the node.
///
/// A fetch can either be:
///   - [`ActiveFetch`]: meaning it is currently being fetched from another node on the network
///   - [`QueuedFetch`]: meaning it is expected to be fetched from a given node, but the
///     repository is already being fetched, or the node is at capacity.
///
/// For any given repository, identified by its [`RepoId`], there can only be
/// one fetch occurring for it at a given time. This prevents any concurrent
/// fetches from clobbering overlapping references.
///
/// If the repository is actively being fetched, then that fetch will be queued
/// for a later attempt.
///
/// For any given node, there is a configurable capacity so that only `N` number
/// of fetches can happen with it concurrently. This does not guarantee that the
/// node will actually allow this node to fetch from it – since it will maintain
/// its own capacity for connections and load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetcherState {
    /// The active fetches that are occurring, ensuring only one fetch per repository.
    active: BTreeMap<RepoId, ActiveFetch>,
    /// The queued fetches, waiting to happen, where each node maintains its own queue.
    queues: BTreeMap<NodeId, Queue>,
    /// Configuration for maintaining the fetch state.
    config: Config,
}

impl Default for FetcherState {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl FetcherState {
    /// Initialize the [`FetcherState`] with the given [`Config`].
    pub fn new(config: Config) -> Self {
        Self {
            active: BTreeMap::new(),
            queues: BTreeMap::new(),
            config,
        }
    }
}

impl FetcherState {
    /// Process the handling of a [`Command`], delegating to its corresponding
    /// method, and returning the corresponding [`Event`].
    ///
    /// This method is useful if the [`FetcherState`] is used in batch
    /// processing and does need to be explicit about the underlying method.
    pub fn handle(&mut self, command: Command) -> Event {
        match command {
            Command::Fetch(fetch) => self.fetch(fetch).into(),
            Command::Fetched(fetched) => self.fetched(fetched).into(),
            Command::Cancel(cancel) => self.cancel(cancel).into(),
        }
    }

    /// Process a [`Fetch`] command, which transitions the given fetch to
    /// active, if possible.
    ///
    /// The fetch will only transition to being active if:
    ///
    ///   - A fetch is not already happening for that repository, in which case it gets queued.
    ///   - The node to be fetched from is not already at capacity, again it will be queued.
    ///
    /// [`Fetch`]: command::Fetch
    pub fn fetch(
        &mut self,
        command::Fetch {
            from,
            rid,
            refs,
            timeout,
        }: command::Fetch,
    ) -> event::Fetch {
        if let Some(active) = self.active.get(&rid) {
            if active.refs == refs && active.from == from {
                return event::Fetch::AlreadyFetching { rid, from };
            } else {
                return self.enqueue(rid, from, refs, timeout);
            }
        }

        if self.is_at_node_capacity(&from) {
            self.enqueue(rid, from, refs, timeout)
        } else {
            self.active.insert(
                rid,
                ActiveFetch {
                    from,
                    refs: refs.clone(),
                },
            );
            event::Fetch::Started {
                rid,
                from,
                refs,
                timeout,
            }
        }
    }

    /// Process a [`Fetched`] command, which removes the given fetch from the set of active fetches.
    /// Note that this is agnostic of whether the fetch succeeded or failed.
    ///
    /// The caller will be notified if the completed fetch did not exist in the active set.
    ///
    /// [`Fetched`]: command::Fetched
    pub fn fetched(&mut self, command::Fetched { from, rid }: command::Fetched) -> event::Fetched {
        match self.active.remove(&rid) {
            None => event::Fetched::NotFound { from, rid },
            Some(ActiveFetch { from, refs }) => event::Fetched::Completed { from, rid, refs },
        }
    }

    /// Attempt to dequeue a [`QueuedFetch`] for the given node.
    ///
    /// This will only dequeue the fetch if it is not active, and the given node
    /// is not at capacity.
    pub fn dequeue(&mut self, from: &NodeId) -> Option<QueuedFetch> {
        let is_at_capacity = self.is_at_node_capacity(from);
        let queue = self.queues.get_mut(from)?;
        let active = &self.active;
        queue.try_dequeue(|QueuedFetch { rid, .. }| !is_at_capacity && !active.contains_key(rid))
    }

    /// Process a [`Cancel`] command, which cancels any active and/or queued
    /// fetches for that given node.
    ///
    /// [`Cancel`]: command::Cancel
    pub fn cancel(&mut self, command::Cancel { from }: command::Cancel) -> event::Cancel {
        let cancelled: Vec<_> = self
            .active
            .iter()
            .filter_map(|(rid, f)| (f.from == from).then_some(*rid))
            .collect();
        let ongoing: BTreeMap<_, _> = cancelled
            .iter()
            .filter_map(|rid| self.active.remove(rid).map(|f| (*rid, f)))
            .collect();
        let ongoing = (!ongoing.is_empty()).then_some(ongoing);
        let queued = self.queues.remove(&from).filter(|queue| !queue.is_empty());

        match (ongoing, queued) {
            (None, None) => event::Cancel::Unexpected { from },
            (ongoing, queued) => event::Cancel::Canceled {
                from,
                active: ongoing.unwrap_or_default(),
                queued: queued.map(|q| q.queue).unwrap_or_default(),
            },
        }
    }

    fn enqueue(
        &mut self,
        rid: RepoId,
        from: NodeId,
        refs: RefsToFetch,
        timeout: time::Duration,
    ) -> event::Fetch {
        let queue = self
            .queues
            .entry(from)
            .or_insert(Queue::new(self.config.maximum_queue_size));
        match queue.enqueue(QueuedFetch { rid, refs, timeout }) {
            Enqueue::CapacityReached(QueuedFetch { rid, refs, timeout }) => {
                event::Fetch::QueueAtCapacity {
                    rid,
                    from,
                    refs,
                    timeout,
                    capacity: queue.len(),
                }
            }
            Enqueue::Queued => event::Fetch::Queued { rid, from },
            Enqueue::Merged => event::Fetch::Queued { rid, from },
        }
    }
}

impl FetcherState {
    /// Get the set of queued fetches.
    pub fn queued_fetches(&self) -> &BTreeMap<NodeId, Queue> {
        &self.queues
    }

    /// Get the set of active fetches.
    pub fn active_fetches(&self) -> &BTreeMap<RepoId, ActiveFetch> {
        &self.active
    }

    /// Get the [`ActiveFetch`] for the provided [`RepoId`], returning `None` if
    /// it does not exist.
    pub fn get_active_fetch(&self, rid: &RepoId) -> Option<&ActiveFetch> {
        self.active.get(rid)
    }

    /// Check if the number of fetches exceeds the maximum number of concurrent
    /// fetches for a given [`NodeId`].
    ///
    /// Returns `true` if the fetcher is fetching the maximum number of
    /// repositories, for that node.
    fn is_at_node_capacity(&self, node: &NodeId) -> bool {
        let count = self.active.values().filter(|f| &f.from == node).count();
        count >= self.config.maximum_concurrency.into()
    }
}

/// Configuration for the [`FetcherState`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Config {
    /// Maximum number of concurrent fetches per peer connection.
    maximum_concurrency: NonZeroUsize,
    /// Maximum fetching queue size for a single node.
    maximum_queue_size: MaxQueueSize,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    /// Maximum fetching queue size for a single node.
    pub fn with_max_capacity(mut self, capacity: MaxQueueSize) -> Self {
        self.maximum_queue_size = capacity;
        self
    }

    /// Maximum number of concurrent fetches per peer connection.
    pub fn with_max_concurrency(mut self, concurrency: NonZeroUsize) -> Self {
        self.maximum_concurrency = concurrency;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            maximum_concurrency: MAX_CONCURRENCY,
            maximum_queue_size: MaxQueueSize::default(),
        }
    }
}

/// An active fetch represents a repository being fetched by a particular node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveFetch {
    pub from: NodeId,
    pub refs: RefsToFetch,
}

impl ActiveFetch {
    /// The node from which the repository is being fetched.
    pub fn from(&self) -> &NodeId {
        &self.from
    }

    /// The set of references that fetch is being performed for.
    pub fn refs(&self) -> &RefsToFetch {
        &self.refs
    }
}

/// A fetch that is waiting to be processed, in the fetch queue.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueuedFetch {
    /// The repository that will be fetched.
    pub rid: RepoId,
    /// The references that the fetch is being performed for.
    pub refs: RefsToFetch,
    /// The timeout given for the fetch request.
    pub timeout: time::Duration,
}

/// A queue for keeping track of fetches.
///
/// It ensures that the queue contains unique items for fetching, and does not
/// exceed the provided maximum capacity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Queue {
    queue: VecDeque<QueuedFetch>,
    max_queue_size: MaxQueueSize,
}

/// The maximum number of fetches that can be queued for a single node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaxQueueSize(usize);

impl MaxQueueSize {
    /// Minimum queue size is `1`.
    pub const MIN: Self = MaxQueueSize(1);

    /// Create a queue size, that must be larger than `0`.
    pub fn new(size: NonZeroUsize) -> Self {
        Self(size.into())
    }

    pub fn as_usize(&self) -> usize {
        self.0
    }

    /// Checks if the `n` provided exceeds the maximum queue size.
    fn is_exceeded_by(&self, n: usize) -> bool {
        n >= self.0
    }
}

impl Default for MaxQueueSize {
    fn default() -> Self {
        Self(MAX_FETCH_QUEUE_SIZE)
    }
}

/// The result of [`Queue::enqueue`].
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Enqueue {
    /// The capacity of the queue has been exceeded, and the [`QueuedFetch`] is
    /// returned.
    CapacityReached(QueuedFetch),
    /// The [`QueuedFetch`] was successfully queued.
    Queued,
    Merged,
}

impl Queue {
    /// Create the [`Queue`] with the given [`MaxQueueSize`].
    pub(super) fn new(max_queue_size: MaxQueueSize) -> Self {
        Self {
            queue: VecDeque::with_capacity(max_queue_size.0),
            max_queue_size,
        }
    }

    /// The current number of items in the queue.
    pub(super) fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns `true` if the [`Queue`] is empty.
    pub(super) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Enqueues a fetch onto the back of the queue, and will only succeed if
    /// the queue has not reached capacity and if the item is unique.
    pub(super) fn enqueue(&mut self, fetch: QueuedFetch) -> Enqueue {
        if let Some(existing) = self.queue.iter_mut().find(|qf| qf.rid == fetch.rid) {
            existing.refs = existing.refs.clone().merge(fetch.refs);
            // Take the longer timeout (more generous)
            existing.timeout = existing.timeout.max(fetch.timeout);
            return Enqueue::Merged;
        }

        if self.max_queue_size.is_exceeded_by(self.queue.len()) {
            Enqueue::CapacityReached(fetch)
        } else {
            self.queue.push_back(fetch);
            Enqueue::Queued
        }
    }

    /// Try to dequeue the next [`QueuedFetch`], but only if the `predicate`
    /// holds, otherwise it will be pushed back to the front of the queue.
    pub(super) fn try_dequeue<P>(&mut self, predicate: P) -> Option<QueuedFetch>
    where
        P: FnOnce(&QueuedFetch) -> bool,
    {
        let fetch = self.dequeue()?;
        if predicate(&fetch) {
            Some(fetch)
        } else {
            self.queue.push_front(fetch);
            None
        }
    }

    /// Dequeues a fetch from the front of the queue.
    pub(super) fn dequeue(&mut self) -> Option<QueuedFetch> {
        self.queue.pop_front()
    }

    /// Return an iterator over the queued fetches.
    pub fn iter<'a>(&'a self) -> QueueIter<'a> {
        QueueIter {
            inner: self.queue.iter(),
        }
    }
}

/// Iterator of the [`QueuedFetch`]'s
pub struct QueueIter<'a> {
    inner: std::collections::vec_deque::Iter<'a, QueuedFetch>,
}

impl<'a> Iterator for QueueIter<'a> {
    type Item = &'a QueuedFetch;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a> IntoIterator for &'a Queue {
    type Item = &'a QueuedFetch;
    type IntoIter = QueueIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
