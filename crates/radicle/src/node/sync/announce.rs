use std::{
    collections::{BTreeMap, BTreeSet},
    ops::ControlFlow,
    time,
};

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

        // N.b extend the unsynced set with any preferred seeds that are not yet
        // synced
        config
            .unsynced
            .extend(config.preferred_seeds.difference(&config.synced).copied());

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
        let SuccessCounts { preferred, synced } = self.success_counts();
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
        let SuccessCounts { preferred, synced } = self.success_counts();
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
#[derive(Debug)]
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

#[derive(Debug)]
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

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod test {
    use crate::test::arbitrary;

    use super::*;

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
                preferred: 2,
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
                preferred: 2,
                synced: 6,
            }
        )
    }

    #[test]
    fn announcer_must_reach_preferred_seeds() {
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
                preferred: 2,
                synced: 10,
            }
        )
    }

    #[test]
    fn announcer_will_minimise_replication_factor() {
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
        let mut success = None;
        let mut successes = 0;

        // Simulate not being able to reach all nodes
        for node in to_sync.iter() {
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
                preferred: 2,
                synced: 10,
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
}
