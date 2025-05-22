use std::{
    collections::{BTreeMap, BTreeSet},
    ops::ControlFlow,
    time,
};

use crate::node::NodeId;

use super::{PrivateNetwork, ReplicationFactor};

pub struct Announcer {
    local_node: NodeId,
    target: Target,
    synced: BTreeMap<NodeId, SyncStatus>,
    to_sync: BTreeSet<NodeId>,
}

impl Announcer {
    /// Construct a new [`Announcer`] from the [`AnnouncerConfig`].
    ///
    /// This will ensure that the local [`NodeId`], provided in the
    /// [`AnnouncerConfig`], will be removed from all sets.
    ///
    /// # Errors
    ///
    /// Returns the following errors:
    ///
    ///   - [`AnnouncerError::NoSeeds`]: both sets of already synchronized and
    ///     un-synchronized nodes were empty
    ///     of nodes were empty
    ///   - [`AnnouncerError::AlreadySynced`]: no more nodes are available for
    ///     synchronizing with
    ///   - [`AnnouncerError::Target`]: the target has no preferred seeds and no
    ///     replicas
    pub fn new(mut config: AnnouncerConfig) -> Result<Self, AnnouncerError> {
        // N.b. ensure that local node is none of the sets
        config.preferred_seeds.remove(&config.local_node);
        config.synced.remove(&config.local_node);
        config.unsynced.remove(&config.local_node);

        if config.synced.is_empty() && config.unsynced.is_empty() {
            return Err(AnnouncerError::NoSeeds);
        }

        if config.unsynced.is_empty() {
            let preferred = config.synced.intersection(&config.preferred_seeds).count();
            return Err(AlreadySynced {
                preferred,
                synced: config.synced.len(),
            }
            .into());
        }

        let replicas = config.replicas.min(config.unsynced.len());
        let announcer = Self {
            local_node: config.local_node,
            target: Target::new(config.preferred_seeds, replicas)
                .map_err(AnnouncerError::Target)?,
            synced: config
                .synced
                .into_iter()
                .map(|nid| (nid, SyncStatus::AlreadySynced))
                .collect(),
            to_sync: config.unsynced,
        };
        match announcer.is_target_reached() {
            None => Ok(announcer),
            Some(outcome) => match outcome {
                SuccessfulOutcome::MinReplicationFactor { preferred, synced } => {
                    Err(AlreadySynced { preferred, synced }.into())
                }
                SuccessfulOutcome::MaxReplicationFactor { preferred, synced } => {
                    Err(AlreadySynced { preferred, synced }.into())
                }
            },
        }
    }

    /// Mark the `node` as synchronized, with the given `duration` it took to
    /// synchronize with.
    ///
    /// If the target for the [`Announcer`] has been reached, then a [`Success`] is
    /// returned via [`ControlFlow::Break`]. Otherwise, [`Progress`] is returned
    /// via [`ControlFlow::Continue`].
    ///
    /// The caller decides whether they wish to continue the announcement process.
    pub fn synced_with(
        &mut self,
        node: NodeId,
        duration: time::Duration,
    ) -> ControlFlow<Success, Progress> {
        if node == self.local_node {
            return ControlFlow::Continue(self.progress());
        }
        self.to_sync.remove(&node);
        self.synced.insert(node, SyncStatus::Synced { duration });
        self.finished()
    }

    /// Complete the [`Announcer`] process returning a [`AnnouncerResult`].
    ///
    /// If the target for the [`Announcer`] has been reached, then the result
    /// will be [`AnnouncerResult::Success`], otherwise, it will be
    /// [`AnnouncerResult::TimedOut`].
    pub fn timed_out(self) -> AnnouncerResult {
        match self.is_target_reached() {
            None => TimedOut {
                synced: self.synced,
                timed_out: self.to_sync,
            }
            .into(),
            Some(outcome) => Success {
                outcome,
                synced: self.synced,
            }
            .into(),
        }
    }

    /// Check if the [`Announcer`] can continue synchronizing with more nodes.
    /// If there are no more nodes, then [`NoNodes`] is returned in the
    /// [`ControlFlow::Break`], otherwise the [`Announcer`] is returned as-is in
    /// the [`ControlFlow::Continue`].
    pub fn can_continue(self) -> ControlFlow<NoNodes, Self> {
        if self.to_sync.is_empty() {
            ControlFlow::Break(NoNodes {
                synced: self.synced,
            })
        } else {
            ControlFlow::Continue(self)
        }
    }

    /// Get all the nodes to be synchronized with.
    pub fn to_sync(&self) -> BTreeSet<NodeId> {
        self.to_sync
            .iter()
            .filter(|node| *node != &self.local_node)
            .copied()
            .collect()
    }

    /// Get the [`Target`] of the [`Announcer`].
    pub fn target(&self) -> &Target {
        &self.target
    }

    /// Get the [`Progress`] of the [`Announcer`].
    pub fn progress(&self) -> Progress {
        let (synced, preferred) = self.success_counts();
        let unsynced = self.to_sync.len().saturating_sub(synced);
        Progress {
            preferred,
            synced,
            unsynced,
        }
    }

    fn finished(&self) -> ControlFlow<Success, Progress> {
        let progress = self.progress();
        self.is_target_reached()
            .map_or(ControlFlow::Continue(progress), |outcome| {
                ControlFlow::Break(Success {
                    outcome,
                    synced: self.synced.clone(),
                })
            })
    }

    fn is_target_reached(&self) -> Option<SuccessfulOutcome> {
        let (preferred, synced) = self.success_counts();
        let reached_preferred = self.target.preferred_seeds.is_empty()
            || preferred >= self.target.preferred_seeds.len();

        let replicas = self.target.replicas();
        let min = replicas.lower_bound();
        match replicas.upper_bound() {
            None => (reached_preferred && synced >= min)
                .then_some(SuccessfulOutcome::MinReplicationFactor { preferred, synced }),
            Some(max) => (reached_preferred && synced >= max)
                .then_some(SuccessfulOutcome::MaxReplicationFactor { preferred, synced }),
        }
    }

    fn success_counts(&self) -> (usize, usize) {
        self.synced
            .keys()
            .fold((0, 0), |(mut preferred, mut succeeded), nid| {
                succeeded += 1;
                if self.target.preferred_seeds.contains(nid) {
                    preferred += 1;
                }
                (preferred, succeeded)
            })
    }
}

/// Configuration of the [`Announcer`].
pub struct AnnouncerConfig {
    local_node: NodeId,
    replicas: ReplicationFactor,
    preferred_seeds: BTreeSet<NodeId>,
    synced: BTreeSet<NodeId>,
    unsynced: BTreeSet<NodeId>,
}

impl AnnouncerConfig {
    /// Setup a private network `AnnouncerConfig`, populating the
    /// [`AnnouncerConfig`]'s preferred seeds with the allowed set from the
    /// [`PrivateNetwork`].
    ///
    /// `replicas` is the target number of seeds the [`Announcer`] should reach
    /// before stopping.
    ///
    /// `local` is the [`NodeId`] of the local node, to ensure it is
    /// excluded from the [`Announcer`] process.
    pub fn private(local: NodeId, replicas: ReplicationFactor, network: PrivateNetwork) -> Self {
        AnnouncerConfig {
            local_node: local,
            replicas,
            preferred_seeds: network.allowed.clone(),
            synced: BTreeSet::new(),
            unsynced: network.allowed,
        }
    }

    /// Setup a public `AnnouncerConfig`.
    ///
    /// `preferred_seeds` is the target set of preferred seeds that [`Announcer`] should
    /// attempt to synchronize with.
    ///
    /// `synced` and `unsynced` are the set of nodes that are currently
    /// synchronized and un-synchronized with, respectively.
    ///
    /// `replicas` is the target number of seeds the [`Announcer`] should reach
    /// before stopping.
    ///
    /// `local` is the [`NodeId`] of the local node, to ensure it is
    /// excluded from the [`Announcer`] process.
    pub fn public(
        local: NodeId,
        replicas: ReplicationFactor,
        preferred_seeds: BTreeSet<NodeId>,
        synced: BTreeSet<NodeId>,
        unsynced: BTreeSet<NodeId>,
    ) -> Self {
        Self {
            local_node: local,
            replicas,
            preferred_seeds,
            synced,
            unsynced,
        }
    }
}

/// Result of running an [`Announcer`] process.
pub enum AnnouncerResult {
    /// The target of the [`Announcer`] was successfully met.
    Success(Success),
    /// The [`Announcer`] process was timed out, and all un-synchronized nodes
    /// are marked as timed out.
    ///
    /// Note that some nodes still may have synchronized.
    TimedOut(TimedOut),
    /// The [`Announcer`] ran out of nodes to synchronize with.
    ///
    /// Note that some nodes still may have synchronized.
    NoNodes(NoNodes),
}

impl AnnouncerResult {
    /// Get the synchronized nodes, regardless of the result.
    pub fn synced(&self) -> &BTreeMap<NodeId, SyncStatus> {
        match self {
            AnnouncerResult::Success(Success { synced, .. }) => synced,
            AnnouncerResult::TimedOut(TimedOut { synced, .. }) => synced,
            AnnouncerResult::NoNodes(NoNodes { synced }) => synced,
        }
    }

    /// Check if a given node is synchronized with.
    pub fn is_synced(&self, node: &NodeId) -> bool {
        let synced = self.synced();
        synced.contains_key(node)
    }
}

impl From<Success> for AnnouncerResult {
    fn from(s: Success) -> Self {
        Self::Success(s)
    }
}

impl From<TimedOut> for AnnouncerResult {
    fn from(to: TimedOut) -> Self {
        Self::TimedOut(to)
    }
}

impl From<NoNodes> for AnnouncerResult {
    fn from(no: NoNodes) -> Self {
        Self::NoNodes(no)
    }
}

pub struct NoNodes {
    synced: BTreeMap<NodeId, SyncStatus>,
}

impl NoNodes {
    /// Get the set of synchronized nodes
    pub fn synced(&self) -> &BTreeMap<NodeId, SyncStatus> {
        &self.synced
    }
}

pub struct TimedOut {
    synced: BTreeMap<NodeId, SyncStatus>,
    timed_out: BTreeSet<NodeId>,
}

impl TimedOut {
    /// Get the set of synchronized nodes
    pub fn synced(&self) -> &BTreeMap<NodeId, SyncStatus> {
        &self.synced
    }

    /// Get the set of timed out nodes
    pub fn timed_out(&self) -> &BTreeSet<NodeId> {
        &self.timed_out
    }
}

pub struct Success {
    outcome: SuccessfulOutcome,
    synced: BTreeMap<NodeId, SyncStatus>,
}

impl Success {
    /// Get the [`SuccessfulOutcome`] of the success.
    pub fn outcome(&self) -> SuccessfulOutcome {
        self.outcome
    }

    /// Get the set of synchronized nodes.
    pub fn synced(&self) -> &BTreeMap<NodeId, SyncStatus> {
        &self.synced
    }
}

/// Error in constructing the [`Announcer`].
pub enum AnnouncerError {
    /// Both sets of already synchronized and un-synchronized nodes were empty
    /// of nodes were empty.
    AlreadySynced(AlreadySynced),
    /// No more nodes are available for synchronizing with.
    NoSeeds,
    /// The target could not be constructed.
    Target(TargetError),
}

impl From<AlreadySynced> for AnnouncerError {
    fn from(value: AlreadySynced) -> Self {
        Self::AlreadySynced(value)
    }
}

pub struct AlreadySynced {
    preferred: usize,
    synced: usize,
}

impl AlreadySynced {
    /// Get the number of preferred nodes that are already synchronized.
    pub fn preferred(&self) -> usize {
        self.preferred
    }

    /// Get the total number of nodes that are already synchronized.
    pub fn synced(&self) -> usize {
        self.synced
    }
}

/// The status of the synchronized node.
#[derive(Clone, Copy, Debug)]
pub enum SyncStatus {
    /// The node was already synchronized before starting the [`Announcer`]
    /// process.
    AlreadySynced,
    /// The node was synchronized as part of the [`Announcer`] process, marking
    /// the amount of time that passed to synchronize with the node.
    Synced { duration: time::Duration },
}

/// Progress of the [`Announcer`] process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    preferred: usize,
    synced: usize,
    unsynced: usize,
}

impl Progress {
    /// The number of preferred seeds that are synchronized.
    pub fn preferred(&self) -> usize {
        self.preferred
    }

    /// The number of seeds that are synchronized.
    pub fn synced(&self) -> usize {
        self.synced
    }

    /// The number of seeds that are un-synchronized.
    pub fn unsynced(&self) -> usize {
        self.unsynced
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
#[error("a minimum number of replicas or set of preferred seeds must be provided")]
pub struct TargetError;

/// The target for the [`Announcer`] to reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    preferred_seeds: BTreeSet<NodeId>,
    replicas: ReplicationFactor,
}

impl Target {
    pub fn new(
        preferred_seeds: BTreeSet<NodeId>,
        replicas: ReplicationFactor,
    ) -> Result<Self, TargetError> {
        if replicas.lower_bound() == 0 && preferred_seeds.is_empty() {
            Err(TargetError)
        } else {
            Ok(Self {
                preferred_seeds,
                replicas,
            })
        }
    }

    /// Get the set of preferred seeds that are trying to be synchronized with.
    pub fn preferred_seeds(&self) -> &BTreeSet<NodeId> {
        &self.preferred_seeds
    }

    /// Get the number of replicas that is trying to be reached.
    pub fn replicas(&self) -> &ReplicationFactor {
        &self.replicas
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuccessfulOutcome {
    MinReplicationFactor { preferred: usize, synced: usize },
    MaxReplicationFactor { preferred: usize, synced: usize },
}
