pub mod commands;
pub use commands::Command;
pub mod effects;
pub mod events;
pub mod fetch_announced;

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::time;

use nonempty::NonEmpty;

use radicle::identity::DocAt;
use radicle::node;
use radicle::node::NodeId;
use radicle::prelude::RepoId;
use radicle::storage::{refs::RefsAt, RefUpdate};

// TODO(finto): I think this should be defined here, and the worker should
// provide the result based on this.
type WorkerResult = Result<Fetched, crate::worker::FetchError>;

// TODO(finto): `Service::fetch_refs_at` and the use of `refs_status_of` is a
// layer above the `Fetcher` where it would perform I/O, mocked out by a trait,
// to check if there are wants and add a fetch to the Fetcher.

/// Maximum items in the fetch queue.
pub const MAX_FETCH_QUEUE_SIZE: usize = 128;

/// Maintain the state of ongoing and queued fetches.
pub struct FetcherState {
    fetching: BTreeMap<NodeId, Fetching>,
    queues: BTreeMap<NodeId, Queue>,
    config: Config,
}

impl FetcherState {
    /// Transition the state of ongoing fetches by recording new fetches that
    /// are occurring, queuing them is the node if required, attempting to
    /// dequeue fetches, and marking fetches as complete.
    pub fn handle_command(&mut self, command: Command) -> Vec<Command> {
        match command {
            Command::Fetch(fetch) => self.handle_fetch(fetch),
            Command::Fetched(fetched) => self.handle_fetched(fetched),
        }
    }

    fn handle_fetch(&mut self, fetch: commands::Fetch) -> Vec<Command> {
        let mut commands = Vec::new();
        match fetch {
            commands::Fetch::Repository { from, rid, .. } => {
                let fetching = self.fetching.entry(from).or_default();
                fetching.fetching(
                    rid,
                    FetchingFor {
                        from,
                        refs_at: vec![],
                    },
                );
            }
            commands::Fetch::RefsAt {
                from, rid, refs_at, ..
            } => {
                let fetching = self.fetching.entry(from).or_default();
                fetching.fetching(
                    rid,
                    FetchingFor {
                        from,
                        refs_at: refs_at.into(),
                    },
                );
            }
            commands::Fetch::Queue {
                from,
                rid,
                refs_at,
                timeout,
            } => {
                let queue = self
                    .queues
                    .entry(from)
                    .or_insert(Queue::new(self.config.maximum_queue_size));
                match queue.enqueue(QueuedFetch {
                    rid,
                    from,
                    refs_at,
                    timeout,
                }) {
                    Enqueue::CapacityReached(queued_fetch) => {
                        commands.push(commands::Fetch::from(queued_fetch).into());
                    }
                    Enqueue::Duplicate(queued_fetch) => {
                        commands.push(commands::Fetch::from(queued_fetch).into());
                    }
                    Enqueue::Queued => { /* TODO(finto): I think we also want to return events */ }
                }
            }
        }
        commands
    }

    fn handle_fetched(&mut self, fetched: commands::Fetched) -> Vec<Command> {
        let mut commands = Vec::new();
        match fetched {
            commands::Fetched::DequeueFetches => {
                self.deuque_fetches(&mut commands);
                commands
            }
            commands::Fetched::Fetched { from, rid } => {
                self.fetched_from(&from, &rid);
                vec![]
            }
        }
    }

    fn deuque_fetches(&mut self, commands: &mut Vec<Command>) {
        for queue in self.queues.values_mut() {
            if let Some(QueuedFetch {
                rid,
                from,
                refs_at,
                timeout,
            }) = queue.dequeue()
            {
                let command = NonEmpty::from_vec(refs_at).map_or(
                    commands::Fetch::Repository { from, rid, timeout },
                    |refs_at| commands::Fetch::RefsAt {
                        from,
                        rid,
                        refs_at,
                        timeout,
                    },
                );
                commands.push(command.into());
            }
        }
    }

    fn fetched_from(&mut self, node: &NodeId, rid: &RepoId) -> Option<FetchingFor> {
        let fetching = self.fetching.get_mut(node)?;
        fetching.fetched(rid)
    }
}

impl FetcherState {
    /// The protocol wishes to fetch the [`RepoId`] from the given [`NodeId`],
    /// with the specified set of [`RefsAt`].
    ///
    /// This will result in a [`FetchResult`], reporting back to the protocol
    /// what events occurred, which effects should performed, and the commands
    /// to transition the state using [`FetchState::handle_command`].
    pub fn fetch(
        &self,
        rid: RepoId,
        from: NodeId,
        refs_at: Vec<RefsAt>,
        timeout: time::Duration,
    ) -> FetchResult {
        let mut result = FetchResult::default();
        if let Some(fetching) = self.is_fetching(&from, &rid) {
            result.queue_fetch(rid, from, refs_at, timeout);
            result.already_fetching(rid, fetching.clone());
            return result;
        }

        if self.is_at_capacity(&from) {
            result.queue_fetch(rid, from, refs_at, timeout);
            result.capacity_reached();
            return result;
        }

        match NonEmpty::from_vec(refs_at) {
            Some(refs_at) => result.fetch_refs_at(from, rid, refs_at, timeout),
            None => result.fetch_repository(from, rid, timeout),
        }
        result
    }

    /// The protocol wishes to mark the fetch for [`RepoId`] from the given
    /// [`NodeId`] as done, with the specified [`WorkerResult`].
    ///
    /// This will result in a [`FetchedResult`], reporting back to the protocol
    /// what events occurred, which effects should performed, and the commands
    /// to transition the state using [`FetchState::handle_command`].
    pub fn fetched(&self, from: NodeId, rid: RepoId, worker_result: WorkerResult) -> FetchedResult {
        let mut result = FetchedResult::default();
        if self.is_fetching(&from, &rid).is_none() {
            result.unexpected(from, rid);
            return result;
        };
        match worker_result {
            Ok(success) => {
                if success.clone && success.doc.is_public() {
                    result.public_repo(rid);
                }
                if !success.updated.is_empty() && !success.updated.iter().all(|u| u.is_skipped()) {
                    result.announce(rid, success.doc, success.namespaces.clone());
                }
                result.refs_fetched(from, rid, success.updated.clone());
                let node_result = node::FetchResult::Success {
                    updated: success.updated,
                    namespaces: success.namespaces,
                    clone: success.clone,
                };
                result.notify(from, rid, node_result);
            }
            Err(e) => {
                if e.is_timeout() {
                    result.disconnect(from, e.to_string());
                }
                let node_result = node::FetchResult::Failed {
                    reason: e.to_string(),
                };
                result.notify(from, rid, node_result);
            }
        };

        result.dequeue_fetches();
        result
    }

    /// Get the [`FetchingFor`] for the provided [`NodeId`] and [`RepoId`],
    /// returning `None` if it does not exist.
    fn is_fetching(&self, node: &NodeId, rid: &RepoId) -> Option<&FetchingFor> {
        self.fetching.get(node).and_then(|f| f.is_fetching(rid))
    }

    /// Check if the number of fetches exceeds the maximum number of concurrent
    /// fetches for a given [`NodeId`].
    fn is_at_capacity(&self, node: &NodeId) -> bool {
        let number_of_fetches = self.fetching.get(node).map_or(0, |f| f.number_of_fetches());
        number_of_fetches > self.config.maximum_concurrency
    }
}

/// Configuration for the [`FetchState`].
pub struct Config {
    /// Maximum number of concurrent fetches per peer connection.
    maximum_concurrency: usize,
    /// Maximum fetching queue size for a single node.
    maximum_queue_size: MaxQueueSize,
}

/// Fetch state for an ongoing fetch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchingFor {
    /// Node we're fetching from.
    from: NodeId,
    /// What refs we're fetching.
    refs_at: Vec<RefsAt>,
}

impl FetchingFor {
    pub fn new(from: NodeId, refs_at: Vec<RefsAt>) -> Self {
        Self { from, refs_at }
    }
}

/// Keep track of which repositories are being fetched, and which node and
/// `rad/sigrefs` they are fetching.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Fetching {
    inner: BTreeMap<RepoId, FetchingFor>,
}

impl Fetching {
    /// Get the number of fetches currently happening.
    fn number_of_fetches(&self) -> usize {
        self.inner.len()
    }

    /// Inspect the [`FetchingFor`] of the [`RepoId`].
    fn is_fetching(&self, rid: &RepoId) -> Option<&FetchingFor> {
        self.inner.get(rid)
    }

    /// The [`RepoId`] is currently being fetched.
    fn fetching(&mut self, rid: RepoId, state: FetchingFor) {
        self.inner.insert(rid, state);
    }

    /// The [`RepoId`] has been fetched.
    fn fetched(&mut self, rid: &RepoId) -> Option<FetchingFor> {
        self.inner.remove(rid)
    }
}

/// Fetch waiting to be processed, in the fetch queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedFetch {
    /// Repo being fetched.
    pub rid: RepoId,
    /// Peer being fetched from.
    pub from: NodeId,
    /// Refs being fetched.
    pub refs_at: Vec<RefsAt>,
    /// The timeout given for the fetch request.
    pub timeout: time::Duration,
}

impl From<QueuedFetch> for commands::Fetch {
    fn from(
        QueuedFetch {
            rid,
            from,
            refs_at,
            timeout,
        }: QueuedFetch,
    ) -> Self {
        Self::Queue {
            from,
            rid,
            refs_at,
            timeout,
        }
    }
}

/// A queue for keeping track of fetches.
///
/// It ensures that the queue contains unique items for fetching, and does not
/// exceed the provided maximum capacity.
struct Queue {
    queue: VecDeque<QueuedFetch>,
    max_queue_size: MaxQueueSize,
}

/// The maximum number of fetches that can be queued for a single node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MaxQueueSize(usize);

impl MaxQueueSize {
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
pub enum Enqueue {
    /// The capacity of the queue has been exceeded, and the [`QueuedFetch`] is
    /// returned.
    CapacityReached(QueuedFetch),
    /// The [`QueuedFetch`] was a duplicate.
    Duplicate(QueuedFetch),
    /// The [`QueuedFetch`] was successfully queued.
    Queued,
}

impl Queue {
    /// Create the [`Queue`] with the given [`MaxQueueSize`].
    fn new(max_queue_size: MaxQueueSize) -> Self {
        Self {
            queue: VecDeque::with_capacity(max_queue_size.0),
            max_queue_size,
        }
    }

    /// Enqueues a fetch onto the back of the queue, and will only succeed if
    /// the queue has not reached capacity and if the item is unique.
    fn enqueue(&mut self, fetch: QueuedFetch) -> Enqueue {
        if self.max_queue_size.is_exceeded_by(self.queue.len()) {
            Enqueue::CapacityReached(fetch)
        } else if self.queue.contains(&fetch) {
            Enqueue::Duplicate(fetch)
        } else {
            self.queue.push_back(fetch);
            Enqueue::Queued
        }
    }

    /// Dequeues a fetch from the front of the queue.
    fn dequeue(&mut self) -> Option<QueuedFetch> {
        self.queue.pop_front()
    }
}

/// Events, effects, and commands that occur from [`FetchState::fetch`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FetchResult {
    /// Events that can be recorded by the rest of the system.
    pub events: Vec<events::Fetch>,
    /// Commands to progress [`FetchState`].
    pub commands: Vec<commands::Fetch>,
}

impl FetchResult {
    fn capacity_reached(&mut self) {
        self.events.push(events::Fetch::CapacityReached);
    }

    fn already_fetching(&mut self, rid: RepoId, fetching: FetchingFor) {
        self.events
            .push(events::Fetch::AlreadyFetching { rid, fetching });
    }

    fn fetch_repository(&mut self, from: NodeId, rid: RepoId, timeout: time::Duration) {
        self.commands
            .push(commands::Fetch::Repository { from, rid, timeout });
    }

    fn fetch_refs_at(
        &mut self,
        from: NodeId,
        rid: RepoId,
        refs_at: NonEmpty<RefsAt>,
        timeout: time::Duration,
    ) {
        self.commands.push(commands::Fetch::RefsAt {
            from,
            rid,
            refs_at,
            timeout,
        });
    }

    fn queue_fetch(
        &mut self,
        rid: RepoId,
        from: NodeId,
        refs_at: Vec<RefsAt>,
        timeout: time::Duration,
    ) {
        self.commands.push(commands::Fetch::Queue {
            rid,
            from,
            refs_at,
            timeout,
        });
    }
}

/// Events, effects, and commands that occur from [`FetchState::fetched`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FetchedResult {
    /// Events that can be recorded by the rest of the system.
    pub events: Vec<events::Fetched>,
    /// Commands to progress [`FetchState`].
    pub effects: Vec<effects::Fetched>,
    /// Effects that can be performed by the rest of the system.
    pub commands: Vec<commands::Fetched>,
}

impl FetchedResult {
    fn unexpected(&mut self, node: NodeId, rid: RepoId) {
        self.events
            .push(events::Fetched::UnexpectedResult { node, rid });
    }

    fn notify(&mut self, from: NodeId, rid: RepoId, result: node::FetchResult) {
        self.effects
            .push(effects::Fetched::Notify { from, rid, result })
    }

    fn dequeue_fetches(&mut self) {
        self.commands.push(commands::Fetched::DequeueFetches);
    }

    fn disconnect(&mut self, node: NodeId, reason: String) {
        self.effects
            .push(effects::Fetched::Disconnect { node, reason })
    }

    fn announce(&mut self, rid: RepoId, doc: DocAt, namespaces: HashSet<NodeId>) {
        self.effects.push(effects::Fetched::Announce {
            rid,
            doc,
            namespaces,
        })
    }

    fn public_repo(&mut self, rid: RepoId) {
        self.events.push(events::Fetched::PublicRepo { rid });
    }

    fn refs_fetched(&mut self, node: NodeId, rid: RepoId, updated: Vec<RefUpdate>) {
        self.events
            .push(events::Fetched::RefsFetched { node, rid, updated });
    }
}

#[derive(Debug, Clone)]
pub struct Fetched {
    /// The set of updated references.
    pub updated: Vec<RefUpdate>,
    /// The set of remote namespaces that were updated.
    pub namespaces: HashSet<NodeId>,
    /// The fetch was a full clone.
    pub clone: bool,
    /// Identity doc of fetched repository.
    pub doc: DocAt,
}

impl Fetched {
    pub fn new(doc: DocAt) -> Self {
        Self {
            updated: vec![],
            namespaces: HashSet::new(),
            clone: false,
            doc,
        }
    }
}

#[cfg(any(test, feature = "test"))]
impl qcheck::Arbitrary for Fetched {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        Fetched {
            updated: vec![],
            namespaces: HashSet::arbitrary(g),
            clone: bool::arbitrary(g),
            doc: DocAt::arbitrary(g),
        }
    }
}
