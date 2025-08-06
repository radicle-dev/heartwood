use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::ControlFlow;
use std::time;

use crate::node::NodeId;

use super::{PrivateNetwork, ReplicationFactor};

#[derive(Debug)]
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
        // N.b. ensure that local node is in none of the sets
        config.preferred_seeds.remove(&config.local_node);
        config.synced.remove(&config.local_node);
        config.unsynced.remove(&config.local_node);

        // N.b extend the unsynced set with any preferred seeds that are not yet
        // synced
        let unsynced_preferred = config
            .preferred_seeds
            .difference(&config.synced)
            .copied()
            .collect::<BTreeSet<_>>();
        config.unsynced.extend(unsynced_preferred);

        // Ensure that the unsynced set does not contain any of the synced set –
        // we trust that the synced nodes are already synced with
        let to_sync = config
            .unsynced
            .difference(&config.synced)
            .copied()
            .collect::<BTreeSet<_>>();

        if config.synced.is_empty() && to_sync.is_empty() {
            return Err(AnnouncerError::NoSeeds);
        }

        if to_sync.is_empty() {
            let preferred = config.synced.intersection(&config.preferred_seeds).count();
            return Err(AlreadySynced {
                preferred,
                synced: config.synced.len(),
            }
            .into());
        }

        let replicas = config.replicas.min(to_sync.len());
        let announcer = Self {
            local_node: config.local_node,
            target: Target::new(config.preferred_seeds, replicas)
                .map_err(AnnouncerError::Target)?,
            synced: config
                .synced
                .into_iter()
                .map(|nid| (nid, SyncStatus::AlreadySynced))
                .collect(),
            to_sync,
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
                SuccessfulOutcome::PreferredNodes {
                    preferred,
                    total_nodes_synced,
                } => Err(AlreadySynced {
                    preferred,
                    synced: total_nodes_synced,
                }
                .into()),
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
    // TODO(finto): I'm not sure this is needed with the change to the target
    // logic. Since we can reach the replication factor OR the preferred seeds,
    // AND the replication factor is always capped to the maximum number of
    // seeds to sync with, I don't think we can ever reach a case where
    // `can_continue` hits the `Break`.
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
        let SuccessCounts { preferred, synced } = self.success_counts();
        let unsynced = self.to_sync.len();
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
        // It should not be possible to construct a target that has no preferred
        // seeds and set the target to 0
        debug_assert!(self.target.has_preferred_seeds() || self.target.has_replication_factor());

        let SuccessCounts { preferred, synced } = self.success_counts();
        if self.target.has_preferred_seeds() && preferred >= self.target.preferred_seeds.len() {
            Some(SuccessfulOutcome::PreferredNodes {
                preferred: self.target.preferred_seeds.len(),
                total_nodes_synced: synced,
            })
        } else {
            // The only target to hit is preferred seeds
            if !self.target.has_replication_factor() {
                return None;
            }
            let replicas = self.target.replicas();
            let min = replicas.lower_bound();
            match replicas.upper_bound() {
                None => (synced >= min)
                    .then_some(SuccessfulOutcome::MinReplicationFactor { preferred, synced }),
                Some(max) => (synced >= max)
                    .then_some(SuccessfulOutcome::MaxReplicationFactor { preferred, synced }),
            }
        }
    }

    fn success_counts(&self) -> SuccessCounts {
        self.synced
            .keys()
            .fold(SuccessCounts::default(), |counts, nid| {
                if self.target.preferred_seeds.contains(nid) {
                    counts.preferred().synced()
                } else {
                    counts.synced()
                }
            })
    }
}

#[derive(Default)]
struct SuccessCounts {
    preferred: usize,
    synced: usize,
}

impl SuccessCounts {
    fn synced(self) -> Self {
        Self {
            synced: self.synced + 1,
            ..self
        }
    }

    fn preferred(self) -> Self {
        Self {
            preferred: self.preferred + 1,
            ..self
        }
    }
}

/// Configuration of the [`Announcer`].
#[derive(Clone, Debug)]
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
            // TODO(finto): we should check if the seeds are synced with instead
            // of assuming they haven't been yet.
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
#[derive(Debug)]
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

#[derive(Debug)]
pub struct NoNodes {
    synced: BTreeMap<NodeId, SyncStatus>,
}

impl NoNodes {
    /// Get the set of synchronized nodes
    pub fn synced(&self) -> &BTreeMap<NodeId, SyncStatus> {
        &self.synced
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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
#[derive(Debug, PartialEq, Eq)]
pub enum AnnouncerError {
    /// Both sets of already synchronized and un-synchronized nodes were empty
    /// of nodes were empty.
    AlreadySynced(AlreadySynced),
    /// No more nodes are available for synchronizing with.
    NoSeeds,
    /// The target could not be constructed.
    Target(TargetError),
}

impl fmt::Display for AnnouncerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnnouncerError::AlreadySynced(AlreadySynced { preferred, synced }) => write!(
                f,
                "already synchronized with {synced} nodes ({preferred} preferred nodes)"
            ),
            AnnouncerError::NoSeeds => {
                f.write_str("no more nodes are available for synchronizing with")
            }
            AnnouncerError::Target(target_error) => target_error.fmt(f),
        }
    }
}

impl std::error::Error for AnnouncerError {}

impl From<AlreadySynced> for AnnouncerError {
    fn from(value: AlreadySynced) -> Self {
        Self::AlreadySynced(value)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct AlreadySynced {
    /// The number of preferred nodes that are synchronized.
    preferred: usize,
    /// Total number nodes that are synchronized.
    ///
    /// Note that this includes [`AlreadySynced::preferred`].
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
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

    /// Check if the target has preferred seeds
    pub fn has_preferred_seeds(&self) -> bool {
        !self.preferred_seeds.is_empty()
    }

    /// Check that lower bound of the replication is greater than `0`
    pub fn has_replication_factor(&self) -> bool {
        self.replicas.lower_bound() != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuccessfulOutcome {
    MinReplicationFactor {
        preferred: usize,
        synced: usize,
    },
    MaxReplicationFactor {
        preferred: usize,
        synced: usize,
    },
    PreferredNodes {
        preferred: usize,
        total_nodes_synced: usize,
    },
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod test {
    use crate::{assert_matches, test::arbitrary};

    use super::*;

    #[test]
    fn all_synced_nodes_are_preferred_seeds() {
        let local = arbitrary::gen::<NodeId>(0);
        let seeds = arbitrary::set::<NodeId>(5..=5);

        // All preferred seeds, no regular seeds in unsynced
        let preferred_seeds = seeds.iter().take(3).copied().collect::<BTreeSet<_>>();
        let unsynced = preferred_seeds.clone(); // Only preferred seeds to sync with

        let config = AnnouncerConfig::public(
            local,
            // High target that we won't reach with preferred alone
            ReplicationFactor::must_reach(5),
            preferred_seeds.clone(),
            BTreeSet::new(),
            unsynced,
        );

        let mut announcer = Announcer::new(config).unwrap();

        // Sync with all preferred seeds
        let mut synced_count = 0;
        let mut result = None;
        for &node in &preferred_seeds {
            let duration = time::Duration::from_secs(1);
            synced_count += 1;

            match announcer.synced_with(node, duration) {
                ControlFlow::Continue(progress) => {
                    assert_eq!(
                        progress.preferred(),
                        synced_count,
                        "Preferred count should increment for each preferred seed"
                    );
                    assert_eq!(
                        progress.synced(),
                        synced_count,
                        "Total synced should equal preferred since all are preferred"
                    );
                }
                ControlFlow::Break(success) => {
                    result = Some(success);
                    break;
                }
            }
        }
        assert_eq!(
            result.unwrap().outcome(),
            SuccessfulOutcome::PreferredNodes {
                preferred: preferred_seeds.len(),
                total_nodes_synced: preferred_seeds.len()
            },
            "Should succeed with PreferredNodes outcome"
        );
    }

    #[test]
    fn preferred_seeds_already_synced() {
        let local = arbitrary::gen::<NodeId>(0);
        let seeds = arbitrary::set::<NodeId>(6..=6);

        let preferred_seeds = seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        let already_synced = preferred_seeds.clone(); // Preferred seeds already synced
        let regular_unsynced = seeds.iter().skip(2).copied().collect::<BTreeSet<_>>();

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(4),
            preferred_seeds.clone(),
            already_synced.clone(),
            regular_unsynced.clone(),
        );

        assert_eq!(
            Announcer::new(config).err(),
            Some(AnnouncerError::AlreadySynced(AlreadySynced {
                preferred: 2,
                synced: 2
            }))
        );
    }

    #[test]
    fn announcer_reached_min_replication_target() {
        let local = arbitrary::gen::<NodeId>(0);
        let seeds = arbitrary::set::<NodeId>(10..=10);
        let unsynced = seeds.iter().skip(3).copied().collect::<BTreeSet<_>>();
        let preferred_seeds = seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(3),
            preferred_seeds.clone(),
            BTreeSet::new(),
            unsynced.clone(),
        );
        let mut announcer = Announcer::new(config).unwrap();
        let to_sync = announcer.to_sync();
        assert_eq!(to_sync, unsynced.union(&preferred_seeds).copied().collect());

        let mut synced_result = BTreeMap::new();
        let mut success = None;
        let mut successes = 0;

        for node in preferred_seeds.iter().take(1) {
            let t = time::Duration::from_secs(1);
            synced_result.insert(*node, SyncStatus::Synced { duration: t });
            successes += 1;
            match announcer.synced_with(*node, t) {
                ControlFlow::Continue(progress) => {
                    assert_eq!(progress.synced(), successes)
                }
                ControlFlow::Break(stop) => {
                    success = Some(stop);
                    break;
                }
            }
        }

        for node in unsynced.iter() {
            assert_ne!(*node, local);
            let t = time::Duration::from_secs(1);
            synced_result.insert(*node, SyncStatus::Synced { duration: t });
            successes += 1;
            match announcer.synced_with(*node, t) {
                ControlFlow::Continue(progress) => {
                    assert_eq!(progress.synced(), successes)
                }
                ControlFlow::Break(stop) => {
                    success = Some(stop);
                    break;
                }
            }
        }
        assert_eq!(*success.as_ref().unwrap().synced(), synced_result);
        assert_eq!(
            success.as_ref().unwrap().outcome(),
            SuccessfulOutcome::MinReplicationFactor {
                preferred: 1,
                synced: 3,
            }
        )
    }

    #[test]
    fn announcer_reached_max_replication_target() {
        let local = arbitrary::gen::<NodeId>(0);
        let seeds = arbitrary::set::<NodeId>(10..=10);
        let unsynced = seeds.iter().skip(3).copied().collect::<BTreeSet<_>>();
        let preferred_seeds = seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::range(3, 6),
            preferred_seeds.clone(),
            BTreeSet::new(),
            unsynced.clone(),
        );
        let mut announcer = Announcer::new(config).unwrap();
        let to_sync = announcer.to_sync();
        assert_eq!(to_sync, unsynced.union(&preferred_seeds).copied().collect());

        let mut synced_result = BTreeMap::new();
        let mut success = None;
        let mut successes = 0;

        // Don't sync with preferred so that we don't hit that target.
        for node in unsynced.iter() {
            assert_ne!(*node, local);
            let t = time::Duration::from_secs(1);
            synced_result.insert(*node, SyncStatus::Synced { duration: t });
            successes += 1;
            match announcer.synced_with(*node, t) {
                ControlFlow::Continue(progress) => {
                    assert_eq!(progress.synced(), successes)
                }
                ControlFlow::Break(stop) => {
                    success = Some(stop);
                    break;
                }
            }
        }
        assert_eq!(*success.as_ref().unwrap().synced(), synced_result);
        assert_eq!(
            success.as_ref().unwrap().outcome(),
            SuccessfulOutcome::MaxReplicationFactor {
                preferred: 0,
                synced: 6,
            }
        )
    }

    #[test]
    fn announcer_preferred_seeds_or_replica_factor() {
        let local = arbitrary::gen::<NodeId>(0);
        let seeds = arbitrary::set::<NodeId>(10..=10);
        let unsynced = seeds.iter().skip(2).copied().collect::<BTreeSet<_>>();
        let preferred_seeds = seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::range(3, 6),
            preferred_seeds.clone(),
            BTreeSet::new(),
            unsynced.clone(),
        );
        let mut announcer = Announcer::new(config).unwrap();
        let to_sync = announcer.to_sync();
        assert_eq!(to_sync, unsynced.union(&preferred_seeds).copied().collect());

        let mut synced_result = BTreeMap::new();
        let mut success = None;
        let mut successes = 0;

        // Reaches max replication factor, and stops.
        for node in unsynced.iter() {
            assert_ne!(*node, local);
            let t = time::Duration::from_secs(1);
            synced_result.insert(*node, SyncStatus::Synced { duration: t });
            successes += 1;
            match announcer.synced_with(*node, t) {
                ControlFlow::Continue(progress) => {
                    assert_eq!(progress.synced(), successes)
                }
                ControlFlow::Break(stop) => {
                    success = Some(stop);
                    break;
                }
            }
        }
        // If we try to continue to drive it forward, we get the extra sync of
        // the preferred seed, but it stops immediately.
        for node in preferred_seeds.iter() {
            let t = time::Duration::from_secs(1);
            synced_result.insert(*node, SyncStatus::Synced { duration: t });
            successes += 1;
            match announcer.synced_with(*node, t) {
                ControlFlow::Continue(progress) => {
                    assert_eq!(progress.synced(), successes)
                }
                ControlFlow::Break(stop) => {
                    success = Some(stop);
                    break;
                }
            }
        }

        assert_eq!(*success.as_ref().unwrap().synced(), synced_result);
        assert_eq!(
            success.as_ref().unwrap().outcome(),
            SuccessfulOutcome::MaxReplicationFactor {
                preferred: 1,
                synced: 7,
            }
        )
    }

    #[test]
    fn announcer_reached_preferred_seeds() {
        let local = arbitrary::gen::<NodeId>(0);
        let seeds = arbitrary::set::<NodeId>(10..=10);
        let unsynced = seeds.iter().skip(2).copied().collect::<BTreeSet<_>>();
        let preferred_seeds = seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(11),
            preferred_seeds.clone(),
            BTreeSet::new(),
            unsynced.clone(),
        );
        let mut announcer = Announcer::new(config).unwrap();

        let mut synced_result = BTreeMap::new();
        let mut success = None;
        let mut successes = 0;

        // The preferred seeds then sync, allowing us to reach that part of the
        // target
        for node in preferred_seeds.iter() {
            assert_ne!(*node, local);
            let t = time::Duration::from_secs(1);
            synced_result.insert(*node, SyncStatus::Synced { duration: t });
            successes += 1;
            match announcer.synced_with(*node, t) {
                ControlFlow::Continue(progress) => {
                    assert_eq!(progress.synced(), successes)
                }
                ControlFlow::Break(stop) => {
                    success = Some(stop);
                    break;
                }
            }
        }

        assert_eq!(*success.as_ref().unwrap().synced(), synced_result);
        assert_eq!(
            success.as_ref().unwrap().outcome(),
            SuccessfulOutcome::PreferredNodes {
                preferred: 2,
                total_nodes_synced: 2,
            }
        )
    }

    #[test]
    fn announcer_timed_out() {
        let local = arbitrary::gen::<NodeId>(0);
        let seeds = arbitrary::set::<NodeId>(10..=10);
        let unsynced = seeds.iter().skip(2).copied().collect::<BTreeSet<_>>();
        let preferred_seeds = seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(11),
            preferred_seeds.clone(),
            BTreeSet::new(),
            unsynced.clone(),
        );
        let mut announcer = Announcer::new(config).unwrap();
        let to_sync = announcer.to_sync();
        assert_eq!(to_sync, unsynced.union(&preferred_seeds).copied().collect());

        let mut synced_result = BTreeMap::new();
        let mut announcer_result = None;
        let mut successes = 0;

        // Simulate not being able to reach all nodes
        for node in to_sync.iter() {
            assert_ne!(*node, local);
            if successes > 5 {
                announcer_result = Some(announcer.timed_out());
                break;
            }
            // Simulate not being able to reach the preferred seeds
            if preferred_seeds.contains(node) {
                continue;
            }
            let t = time::Duration::from_secs(1);
            synced_result.insert(*node, SyncStatus::Synced { duration: t });
            successes += 1;
            match announcer.synced_with(*node, t) {
                ControlFlow::Continue(progress) => {
                    assert_eq!(progress.synced(), successes)
                }
                ControlFlow::Break(stop) => {
                    announcer_result = Some(stop.into());
                    break;
                }
            }
        }

        match announcer_result {
            Some(AnnouncerResult::TimedOut(timeout)) => {
                assert_eq!(timeout.synced, synced_result);
                assert_eq!(
                    timeout.timed_out,
                    to_sync
                        .difference(&synced_result.keys().copied().collect())
                        .copied()
                        .collect()
                );
            }
            unexpected => panic!("Expected AnnouncerResult::TimedOut, found: {unexpected:#?}"),
        }
    }

    #[test]
    fn announcer_adapts_target_to_reach() {
        let local = arbitrary::gen::<NodeId>(0);
        // Only 3 nodes available
        let unsynced = arbitrary::set::<NodeId>(3..=3)
            .into_iter()
            .collect::<BTreeSet<_>>();

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(5), // Want 5 but only have 3
            BTreeSet::new(),
            BTreeSet::new(),
            unsynced.clone(),
        );

        let announcer = Announcer::new(config).unwrap();
        assert_eq!(announcer.target().replicas().lower_bound(), 3);
    }

    #[test]
    fn announcer_with_replication_factor_zero_and_preferred_seeds() {
        let local = arbitrary::gen::<NodeId>(0);
        let seeds = arbitrary::set::<NodeId>(5..=5);

        let preferred_seeds = seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        let unsynced = seeds.iter().skip(2).copied().collect::<BTreeSet<_>>();

        // Zero replication factor but with preferred seeds should work
        let config = AnnouncerConfig::public(
            local,
            // Zero replication factor
            ReplicationFactor::must_reach(0),
            preferred_seeds.clone(),
            BTreeSet::new(),
            unsynced,
        );

        let mut announcer = Announcer::new(config).unwrap();

        // Should succeed immediately when we sync with all preferred seeds
        for &node in &preferred_seeds {
            let duration = time::Duration::from_secs(1);
            match announcer.synced_with(node, duration) {
                ControlFlow::Continue(_) => {} // Continue until all preferred are synced
                ControlFlow::Break(success) => {
                    assert_eq!(
                        success.outcome(),
                        SuccessfulOutcome::PreferredNodes {
                            preferred: preferred_seeds.len(),
                            total_nodes_synced: preferred_seeds.len()
                        },
                        "Should succeed with preferred seeds even with zero replication factor"
                    );
                    return;
                }
            }
        }

        panic!("Should have succeeded with preferred seeds");
    }

    #[test]
    fn announcer_synced_with_unknown_node() {
        let local = arbitrary::gen::<NodeId>(0);
        let seeds = arbitrary::set::<NodeId>(5..=5);

        let unsynced = seeds.iter().take(3).copied().collect::<BTreeSet<_>>();
        let unknown_node = arbitrary::gen::<NodeId>(100); // Node not in any set

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(1),
            BTreeSet::new(),
            BTreeSet::new(),
            unsynced.clone(),
        );

        let mut announcer = Announcer::new(config).unwrap();

        // Try to sync with an unknown node
        let duration = time::Duration::from_secs(1);
        let mut target_reached = false;
        match announcer.synced_with(unknown_node, duration) {
            ControlFlow::Continue(_) => {}
            ControlFlow::Break(success) => {
                target_reached = true;
                assert_eq!(
                    success.outcome(),
                    SuccessfulOutcome::MinReplicationFactor {
                        preferred: 0,
                        synced: 1
                    },
                    "Should be able to reach target with unknown node"
                );
            }
        }

        assert!(target_reached);
        // Verify the unknown node is now in the synced map
        assert!(
            announcer.synced.contains_key(&unknown_node),
            "Unknown node should be added to synced map"
        );
    }

    #[test]
    fn synced_with_same_node_multiple_times() {
        let local = arbitrary::gen::<NodeId>(0);
        let unsynced = arbitrary::set::<NodeId>(3..=3)
            .into_iter()
            .collect::<BTreeSet<_>>();

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(2),
            BTreeSet::new(),
            BTreeSet::new(),
            unsynced.clone(),
        );

        let mut announcer = Announcer::new(config).unwrap();
        let target_node = *unsynced.iter().next().unwrap();

        // First sync with the node
        let duration1 = time::Duration::from_secs(1);
        match announcer.synced_with(target_node, duration1) {
            ControlFlow::Continue(progress) => {
                assert_eq!(progress.synced(), 1, "First sync should count");
                assert_eq!(
                    progress.unsynced(),
                    unsynced.len() - 1,
                    "Should decrease unsynced"
                );
            }
            ControlFlow::Break(_) => panic!("Should not reach target yet"),
        }

        // Sync with the SAME node again with different duration
        let duration2 = time::Duration::from_secs(5);
        let progress_before_duplicate = announcer.progress();
        match announcer.synced_with(target_node, duration2) {
            ControlFlow::Continue(progress) => {
                // Progress should be UNCHANGED since we already synced with this node
                assert_eq!(
                    progress.synced(),
                    progress_before_duplicate.synced(),
                    "Duplicate sync should not change synced count"
                );
                assert_eq!(
                    progress.unsynced(),
                    progress_before_duplicate.unsynced(),
                    "Duplicate sync should not change unsynced count"
                );
            }
            ControlFlow::Break(_) => panic!("Should not reach target with duplicate sync"),
        }

        // Check that the duration was updated to the latest one
        assert_eq!(
            announcer.synced[&target_node],
            SyncStatus::Synced {
                duration: duration2
            },
            "Duplicate sync should update the duration"
        );

        // Verify the node is no longer in to_sync (should have been removed on first sync)
        assert!(
            !announcer.to_sync.contains(&target_node),
            "Node should not be in to_sync after first sync"
        );
    }

    #[test]
    fn timed_out_after_reaching_success() {
        let local = arbitrary::gen::<NodeId>(0);
        let unsynced = arbitrary::set::<NodeId>(3..=3)
            .into_iter()
            .collect::<BTreeSet<_>>();

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(2),
            BTreeSet::new(),
            BTreeSet::new(),
            unsynced.clone(),
        );

        let mut announcer = Announcer::new(config).unwrap();

        // Sync with enough nodes to reach the target
        let mut synced_nodes = BTreeMap::new();
        for node in unsynced {
            let duration = time::Duration::from_secs(1);
            synced_nodes.insert(node, SyncStatus::Synced { duration });

            match announcer.synced_with(node, duration) {
                ControlFlow::Continue(_) => continue,
                ControlFlow::Break(_) => break, // Reached target
            }
        }

        // Now call timed_out even though we reached success
        match announcer.timed_out() {
            AnnouncerResult::Success(success) => {
                // Should return Success since target was reached
                assert_eq!(
                    success.outcome(),
                    SuccessfulOutcome::MinReplicationFactor {
                        preferred: 0,
                        synced: 2
                    },
                    "Should return success outcome even when called via timed_out"
                );
            }
            other => panic!("Expected Success via timed_out, got: {other:?}"),
        }
    }

    #[test]
    fn construct_only_preferred_seeds_provided() {
        // Test: preferred_seeds non-empty, synced and unsynced empty
        // Expected: preferred seeds should be moved to to_sync, constructor succeeds
        let local = arbitrary::gen::<NodeId>(0);
        let preferred_seeds = arbitrary::set::<NodeId>(2..=2)
            .into_iter()
            .collect::<BTreeSet<_>>();

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(1),
            preferred_seeds.clone(),
            BTreeSet::new(),
            BTreeSet::new(),
        );

        let announcer = Announcer::new(config).unwrap();

        // Constructor should move unsynced preferred seeds to to_sync
        assert_eq!(announcer.to_sync, preferred_seeds);
        assert_eq!(announcer.target().preferred_seeds(), &preferred_seeds);
        assert!(announcer.synced.is_empty());
    }

    #[test]
    fn construct_node_appears_in_multiple_input_sets() {
        let local = arbitrary::gen::<NodeId>(0);
        let alice = arbitrary::gen::<NodeId>(1);
        let bob = arbitrary::gen::<NodeId>(2);
        let eve = arbitrary::gen::<NodeId>(3);

        // alice will appear in synced and unsynced
        let synced = [alice].iter().copied().collect::<BTreeSet<_>>();
        let unsynced = [alice, bob, eve].iter().copied().collect::<BTreeSet<_>>();

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(2),
            BTreeSet::new(),
            synced,
            unsynced,
        );

        let announcer = Announcer::new(config).unwrap();

        // synced takes precedence over to_sync when constructing
        assert!(
            announcer.synced.contains_key(&alice),
            "alice should be synced"
        );
        assert!(
            !announcer.to_sync.contains(&alice),
            "alice should not appear in to_sync"
        );
        // bob and eve should appear in to_sync
        assert!(
            announcer.to_sync.contains(&bob) && announcer.to_sync.contains(&eve),
            "Other node should be in to_sync"
        );
    }

    #[test]
    fn cannot_construct_announcer() {
        let local = arbitrary::gen::<NodeId>(0);
        let seeds = arbitrary::set::<NodeId>(10..=10);
        let synced = seeds.iter().take(3).copied().collect::<BTreeSet<_>>();
        let unsynced = seeds.iter().skip(3).copied().collect::<BTreeSet<_>>();
        let preferred_seeds = seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        let replicas = ReplicationFactor::default();
        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::default(),
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
        );
        assert!(matches!(
            Announcer::new(config),
            Err(AnnouncerError::NoSeeds)
        ));

        // No nodes to sync
        let config = AnnouncerConfig::public(
            local,
            replicas,
            preferred_seeds.clone(),
            synced.clone(),
            BTreeSet::new(),
        );
        assert!(matches!(
            Announcer::new(config),
            Err(AnnouncerError::AlreadySynced { .. })
        ));

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(0),
            BTreeSet::new(),
            synced.clone(),
            unsynced.clone(),
        );
        assert!(matches!(
            Announcer::new(config),
            Err(AnnouncerError::Target(_))
        ));

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::MustReach(2),
            preferred_seeds.clone(),
            synced.clone(),
            unsynced.clone(),
        );
        // Min replication factor
        assert!(matches!(
            Announcer::new(config),
            Err(AnnouncerError::AlreadySynced { .. })
        ));
        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::range(2, 3),
            preferred_seeds,
            synced,
            unsynced,
        );
        // Max replication factor
        assert!(matches!(
            Announcer::new(config),
            Err(AnnouncerError::AlreadySynced { .. })
        ));
    }

    #[test]
    fn invariant_progress_should_match_state() {
        let local = arbitrary::gen::<NodeId>(0);
        let seeds = arbitrary::set::<NodeId>(6..=6);

        // Set up: 2 already synced, 4 unsynced initially
        let already_synced = seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        let unsynced = seeds.iter().skip(2).copied().collect::<BTreeSet<_>>();

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(4), // Need 4 total
            BTreeSet::new(),                  // No preferred seeds
            already_synced.clone(),
            unsynced.clone(),
        );

        let mut announcer = Announcer::new(config).unwrap();

        // No progress made, so values should be the same
        assert_eq!(
            announcer.progress().unsynced(),
            announcer.to_sync().len(),
            "Expected unsynced progress to be the same as the number of nodes to sync"
        );

        // Expected: progress.synced() should be the number of already synced nodes
        assert_eq!(
            announcer.progress().synced(),
            already_synced.len(),
            "Initial synced count should equal already synced nodes"
        );

        // Now sync with one node and check progress again
        let first_unsynced = *unsynced.iter().next().unwrap();
        let duration = time::Duration::from_secs(1);

        match announcer.synced_with(first_unsynced, duration) {
            ControlFlow::Continue(progress) => {
                assert_eq!(
                    progress.synced(),
                    already_synced.len() + 1,
                    "Synced count should increase by 1"
                );

                assert_eq!(
                    progress.unsynced(),
                    announcer.to_sync().len(),
                    "Unsynced count should equal remaining to_sync length"
                );

                assert_eq!(
                    progress.unsynced(),
                    unsynced.len() - 1,
                    "Unsynced should be original unsynced count minus nodes we've synced"
                );
            }
            ControlFlow::Break(outcome) => {
                panic!("Should not have reached target yet: {outcome:?}")
            }
        }

        // Invariant:
        // synced nodes + unsynced nodes = progress.synced() + progress.unsynced()
        let final_progress = announcer.progress();
        let expected_total = already_synced.len() + unsynced.len();
        let actual_total = final_progress.synced() + final_progress.unsynced();

        assert_eq!(
            actual_total, expected_total,
            "synced + unsynced should equal the total nodes we started with"
        );
    }

    #[test]
    fn local_node_in_preferred_seeds() {
        let local = arbitrary::gen::<NodeId>(0);
        let other_seeds = arbitrary::set::<NodeId>(5..=5);

        // Include local node in preferred seeds
        let mut preferred_seeds = other_seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        preferred_seeds.insert(local);

        let unsynced = other_seeds.iter().skip(2).copied().collect::<BTreeSet<_>>();

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(3),
            preferred_seeds.clone(),
            BTreeSet::new(),
            unsynced.clone(),
        );

        let announcer = Announcer::new(config).unwrap();

        // Verify local node was removed from target's preferred seeds
        assert!(
            !announcer.target().preferred_seeds().contains(&local),
            "Local node should be removed from preferred seeds in target"
        );

        // Verify local node is not in to_sync
        assert!(
            !announcer.to_sync().contains(&local),
            "Local node should not be in to_sync set"
        );

        // The preferred seeds in the target should be one less than what we passed in
        assert_eq!(
            announcer.target().preferred_seeds().len(),
            preferred_seeds.len() - 1,
            "Target should have local node removed from preferred seeds"
        );
    }

    #[test]
    fn local_node_in_synced_set() {
        let local = arbitrary::gen::<NodeId>(0);
        let other_seeds = arbitrary::set::<NodeId>(5..=5);

        // Include local node in synced set
        let mut synced = other_seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        synced.insert(local);

        let unsynced = other_seeds.iter().skip(2).copied().collect::<BTreeSet<_>>();

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(4),
            BTreeSet::new(),
            synced.clone(),
            unsynced.clone(),
        );

        let announcer = Announcer::new(config).unwrap();

        // Verify local node is not counted in synced nodes
        assert!(
            !announcer.synced.contains_key(&local),
            "Local node should not be in internal synced map"
        );

        // Progress should reflect only the non-local synced nodes
        assert_eq!(
            announcer.progress().synced(),
            synced.len() - 1,
            "Progress should not count local node as synced"
        );
    }

    #[test]
    fn local_node_in_unsynced_set() {
        let local = arbitrary::gen::<NodeId>(0);
        let other_seeds = arbitrary::set::<NodeId>(5..=5);

        let synced = other_seeds.iter().take(2).copied().collect::<BTreeSet<_>>();

        // Include local node in unsynced set
        let mut unsynced = other_seeds.iter().skip(2).copied().collect::<BTreeSet<_>>();
        unsynced.insert(local);

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(4),
            BTreeSet::new(),
            synced.clone(),
            unsynced.clone(),
        );

        let announcer = Announcer::new(config).unwrap();

        // Verify local node is not in to_sync
        assert!(
            !announcer.to_sync().contains(&local),
            "Local node should not be in to_sync set"
        );

        // The internal to_sync should not contain local node
        assert!(
            !announcer.to_sync.contains(&local),
            "Internal to_sync should not contain local node"
        );

        // Progress unsynced count should not include local node
        assert_eq!(
            announcer.progress().unsynced(),
            unsynced.len() - 1,
            "Progress unsynced should not count local node"
        );
    }

    #[test]
    fn local_node_in_multiple_sets() {
        let local = arbitrary::gen::<NodeId>(0);
        let other_seeds = arbitrary::set::<NodeId>(5..=5);

        // Include local node in ALL sets
        let mut preferred_seeds = other_seeds.iter().take(2).copied().collect::<BTreeSet<_>>();
        preferred_seeds.insert(local);

        let mut synced = other_seeds
            .iter()
            .skip(2)
            .take(1)
            .copied()
            .collect::<BTreeSet<_>>();
        synced.insert(local);

        let mut unsynced = other_seeds.iter().skip(3).copied().collect::<BTreeSet<_>>();
        unsynced.insert(local);

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(3),
            preferred_seeds.clone(),
            synced.clone(),
            unsynced.clone(),
        );

        let announcer = Announcer::new(config).unwrap();

        // Verify local node is completely absent from all internal structures
        assert!(
            !announcer.target().preferred_seeds().contains(&local),
            "Local node should be removed from preferred seeds"
        );
        assert!(
            !announcer.synced.contains_key(&local),
            "Local node should not be in synced map"
        );
        assert!(
            !announcer.to_sync().contains(&local),
            "Local node should not be in to_sync"
        );
        assert!(
            !announcer.to_sync.contains(&local),
            "Local node should not be in internal to_sync"
        );

        // Verify counts are correct (excluding local node from all)
        assert_eq!(
            announcer.target().preferred_seeds().len(),
            preferred_seeds.len() - 1
        );
        assert_eq!(announcer.progress().synced(), synced.len() - 1);
        // The unsynced nodes includes the preferred seeds, since they are not
        // in the synced set, and `- 1` from each for the local node
        assert_eq!(
            announcer.progress().unsynced(),
            (unsynced.len() - 1) + (preferred_seeds.len() - 1)
        );
    }

    #[test]
    fn synced_with_local_node_is_ignored() {
        let local = arbitrary::gen::<NodeId>(0);
        let unsynced = arbitrary::set::<NodeId>(3..=3).into_iter().collect();

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(2),
            BTreeSet::new(),
            BTreeSet::new(),
            unsynced,
        );

        let mut announcer = Announcer::new(config).unwrap();
        let initial_progress = announcer.progress();

        // Try to sync with the local node - this should be ignored
        let duration = time::Duration::from_secs(1);
        match announcer.synced_with(local, duration) {
            ControlFlow::Continue(progress) => {
                // Progress should be unchanged
                assert_eq!(
                    progress.synced(),
                    initial_progress.synced(),
                    "Syncing with local node should not change synced count"
                );
                assert_eq!(
                    progress.unsynced(),
                    initial_progress.unsynced(),
                    "Syncing with local node should not change unsynced count"
                );
            }
            ControlFlow::Break(_) => panic!("Should not reach target by syncing with local node"),
        }

        // Verify local node is still not in synced map
        assert!(
            !announcer.synced.contains_key(&local),
            "Local node should not be added to synced map"
        );
    }

    #[test]
    fn local_node_only_in_all_sets_results_in_no_seeds_error() {
        let local = arbitrary::gen::<NodeId>(0);

        // Create sets that contain ONLY the local node
        let preferred_seeds = [local].iter().copied().collect::<BTreeSet<_>>();
        let synced = [local].iter().copied().collect::<BTreeSet<_>>();
        let unsynced = [local].iter().copied().collect::<BTreeSet<_>>();

        let config = AnnouncerConfig::public(
            local,
            ReplicationFactor::must_reach(1),
            preferred_seeds,
            synced,
            unsynced,
        );

        // After removing local node from all sets, we should get NoSeeds error
        assert_matches!(Announcer::new(config), Err(AnnouncerError::NoSeeds));
    }
}
