//! A sans-IO fetching state machine for driving fetch processes.
//!
//! See the documentation of [`Fetcher`] for more details.

use std::collections::{BTreeSet, VecDeque};
use std::ops::ControlFlow;

use crate::identity::Visibility;
use crate::node::{Address, FetchResult, FetchResults, NodeId};
use crate::prelude::Doc;

use super::ReplicationFactor;

/// A [`Fetcher`] describes a machine for driving a fetching process.
///
/// The [`Fetcher`] can be constructed using [`Fetcher::new`], providing a
/// [`FetcherConfig`].
///
/// It builds a [`Target`] that it attempts to reach:
///  * Number of replicas that it should successfully fetch from, where a
///    replica is any seed node that the repository is potentially seeded by.
///  * A set of preferred seeds that it should successfully fetch from.
///
/// If either of these targets are reached, then the fetch process can be
/// considered complete – with preference given to the preferred seeds target.
///
/// To drive the [`Fetcher`], it must be provided with nodes to fetch from.
/// These are added via the [`FetcherConfig`]. Note that the nodes provided are
/// retrieved in the order they are provided.
///
/// Before candidate nodes can be fetched from, the caller needs to mark them as
/// connected to. To get the next available node we call [`Fetcher::next_node`].
/// Once the caller attempts to connect to this node and retrieves its
/// [`Address`], then it can mark it as ready to fetch by calling
/// [`Fetcher::connected`].
///
/// To then retrieve the next available node for fetching, the caller uses
/// [`Fetcher::next_fetch`].
///
/// To mark that fetch as complete, we call [`Fetcher::fetch_complete`], with
/// the result. At this point, the [`Fetcher`] returns a [`ControlFlow`] to let
/// the caller know if they should continue processing nodes, to reach the
/// desired target, or they can exit the loop knowing they have successfully
/// reached the target.
///
/// The caller may also call [`Fetcher::fetch_failed`] to mark a fetch for a
/// given node as failed – this is useful for reasons when the caller cannot
/// connect to the node for fetching.
///
/// Finally, if the caller wishes to exit from the fetching process and get the
/// final set of results, they may call [`Fetcher::finish`].
#[derive(Debug)]
#[must_use]
pub struct Fetcher {
    target: Target,
    fetch_from: VecDeque<Connected>,
    candidates: VecDeque<Candidate>,
    results: FetchResults,
    local_node: NodeId,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FetcherError {
    #[error("no candidate seeds were found to fetch from")]
    NoCandidates,
    #[error(transparent)]
    Target(#[from] TargetError),
}

impl Fetcher {
    /// Construct a new [`Fetcher`] from the [`FetcherConfig`].
    pub fn new(config: FetcherConfig) -> Result<Self, FetcherError> {
        if config.candidates.is_empty() {
            return Err(FetcherError::NoCandidates);
        }
        // N.b. ensure that we can reach the replicas count
        let replicas = config.replicas.min(config.candidates.len());
        Ok(Self {
            target: Target::new(config.seeds, replicas)?,
            fetch_from: VecDeque::new(),
            candidates: config.candidates,
            results: FetchResults::default(),
            local_node: config.local_node,
        })
    }

    /// Get the next candidate [`NodeId`] to attempt connection and/or
    /// retrieving their connection session.
    pub fn next_node(&mut self) -> Option<NodeId> {
        let local_node = self.local_node;
        let results = &self.results;
        let include_node = |node: &NodeId| results.get(node).is_none() && local_node != *node;

        // Find the first candidate that passes the `include_node` filter, or we
        // exhaust the candidate list
        std::iter::from_fn(|| self.candidates.pop_front()).find_map(|c| {
            let node = c.nid();
            include_node(&node).then_some(node)
        })
    }

    /// Get the next [`NodeId`] and [`Address`] for performing a fetch from.
    ///
    /// Note that this [`NodeId`] must have been added to the [`Fetcher`] using
    /// the [`Fetcher::connected`] method.
    pub fn next_fetch(&mut self) -> Option<(NodeId, Address)> {
        self.fetch_from
            .pop_front()
            .map(|Connected { node, addr }| (node, addr))
            .filter(|(node, _)| self.include_node(node))
    }

    /// Mark a fetch as failed for the [`NodeId`], using the provided `reason`.
    pub fn fetch_failed(&mut self, node: NodeId, reason: impl ToString) {
        let reason = reason.to_string();
        self.results.push(node, FetchResult::Failed { reason })
    }

    /// Mark a fetch as complete for the [`NodeId`], with the provided
    /// [`FetchResult`].
    ///
    /// If the target for the [`Fetcher`] has been reached, then a [`Success`] is
    /// returned via [`ControlFlow::Break`]. Otherwise, [`Progress`] is returned
    /// via [`ControlFlow::Continue`].
    ///
    /// The caller decides whether they wish to continue the fetching process.
    pub fn fetch_complete(
        &mut self,
        node: NodeId,
        result: FetchResult,
    ) -> ControlFlow<Success, Progress> {
        self.results.push(node, result);
        self.finished()
    }

    /// Complete the [`Fetcher`] process returning a [`FetcherResult`].
    ///
    /// Which variant of the result is returned is determined by whether the
    /// [`Fetcher`]'s target was reached.
    pub fn finish(self) -> FetcherResult {
        let progress = self.progress();
        match self.is_target_reached() {
            None => {
                let missing = self.missing_seeds();
                FetcherResult::target_error(progress, self.target, self.results, missing)
            }
            Some(outcome) => FetcherResult::target_reached(outcome, progress, self.results),
        }
    }

    /// Mark the `node` as connected, by providing its [`Address`].
    ///
    /// This will prime the `node` for fetching.
    pub fn connected(&mut self, node: NodeId, addr: Address) {
        self.fetch_from.push_back(Connected { node, addr })
    }

    /// Get the latest [`Progress`] of the [`Fetcher`].
    pub fn progress(&self) -> Progress {
        let (preferred, succeeded) = self.success_counts();
        Progress {
            candidate: self.candidates.len(),
            succeeded,
            failed: self.results.failed().count(),
            preferred,
        }
    }

    /// Get the [`Target`] that the [`Fetcher`] is aiming to reach.
    pub fn target(&self) -> &Target {
        &self.target
    }

    fn finished(&self) -> ControlFlow<Success, Progress> {
        let progress = self.progress();
        self.is_target_reached()
            .map_or(ControlFlow::Continue(progress), |outcome| {
                ControlFlow::Break(Success {
                    outcome,
                    progress,
                    results: self.results.clone(),
                })
            })
    }

    fn is_target_reached(&self) -> Option<SuccessfulOutcome> {
        let (preferred, succeeded) = self.success_counts();
        if !self.target.seeds.is_empty() && preferred >= self.target.seeds.len() {
            Some(SuccessfulOutcome::PreferredNodes {
                preferred: self.target.seeds.len(),
            })
        } else {
            let replicas = self.target.replicas();
            let min = replicas.lower_bound();
            match replicas.upper_bound() {
                None => (succeeded >= min).then_some(SuccessfulOutcome::MinReplicas { succeeded }),
                Some(max) => (succeeded >= max).then_some(SuccessfulOutcome::MaxReplicas {
                    succeeded,
                    min,
                    max,
                }),
            }
        }
    }

    /// Ensure that node does not already have a result and is not the local
    /// node.
    fn include_node(&self, node: &NodeId) -> bool {
        self.results.get(node).is_none() && self.local_node != *node
    }

    fn missing_seeds(&self) -> BTreeSet<NodeId> {
        self.target
            .seeds
            .iter()
            .filter(|nid| match self.results.get(nid) {
                Some(r) if !r.is_success() => true,
                None => true,
                _ => false,
            })
            .copied()
            .collect()
    }

    fn success_counts(&self) -> (usize, usize) {
        self.results
            .success()
            .fold((0, 0), |(mut preferred, mut succeeded), (nid, _, _)| {
                succeeded += 1;
                if self.target.seeds.contains(nid) {
                    preferred += 1;
                }
                (preferred, succeeded)
            })
    }
}

/// A set of nodes that form a private network for fetching from.
///
/// This could be the set of allowed nodes for a private repository, using
/// [`PrivateNetwork::private_repo`]
pub struct PrivateNetwork {
    allowed: BTreeSet<NodeId>,
}

impl PrivateNetwork {
    pub fn private_repo(doc: &Doc) -> Option<Self> {
        match doc.visibility() {
            Visibility::Public => None,
            Visibility::Private { allow } => {
                let allowed = doc
                    .delegates()
                    .iter()
                    .chain(allow.iter())
                    .map(|did| *did.as_key())
                    .collect();
                Some(Self { allowed })
            }
        }
    }
}

/// The progress a [`Fetcher`] is making.
#[derive(Clone, Copy, Debug)]
pub struct Progress {
    /// How many candidate nodes are known.
    candidate: usize,
    /// How many fetches succeeded.
    succeeded: usize,
    /// How many fetches failed.
    failed: usize,
    /// How many fetches succeeded from preferred seeds.
    preferred: usize,
}

impl Progress {
    /// Get the number of successful fetches.
    pub fn succeeded(&self) -> usize {
        self.succeeded
    }

    /// Get the number of failed fetches.
    pub fn failed(&self) -> usize {
        self.failed
    }

    /// Get the number of successful fetches from preferred seeds.
    pub fn preferred(&self) -> usize {
        self.preferred
    }

    pub fn candidate(&self) -> usize {
        self.candidate
    }
}

/// The target for the `Fetcher` to reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    seeds: BTreeSet<NodeId>,
    replicas: ReplicationFactor,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
#[error("a minimum number of replicas or set of preferred seeds must be provided")]
pub struct TargetError;

impl Target {
    pub fn new(seeds: BTreeSet<NodeId>, replicas: ReplicationFactor) -> Result<Self, TargetError> {
        if replicas.lower_bound() == 0 && seeds.is_empty() {
            Err(TargetError)
        } else {
            Ok(Self { seeds, replicas })
        }
    }

    /// Get the set of preferred seeds that are trying to be fetched from.
    pub fn preferred_seeds(&self) -> &BTreeSet<NodeId> {
        &self.seeds
    }

    /// Get the number of replicas that is trying to be reached.
    pub fn replicas(&self) -> &ReplicationFactor {
        &self.replicas
    }
}

/// The outcome reached by the [`Fetcher`], depending on which target was
/// reached first.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SuccessfulOutcome {
    PreferredNodes {
        preferred: usize,
    },
    MinReplicas {
        succeeded: usize,
    },
    MaxReplicas {
        succeeded: usize,
        min: usize,
        max: usize,
    },
}

/// A successful `Fetcher` process result, where the target was reached.
pub struct Success {
    outcome: SuccessfulOutcome,
    progress: Progress,
    results: FetchResults,
}

impl Success {
    /// Get the final [`Progress`] of the fetcher result.
    pub fn progress(&self) -> Progress {
        self.progress
    }

    /// Get the final [`FetchResults`] of the fetcher result.
    pub fn fetch_results(&self) -> &FetchResults {
        &self.results
    }

    /// Get the [`SuccessfulOutcome`] of the fetcher result.
    pub fn outcome(&self) -> &SuccessfulOutcome {
        &self.outcome
    }
}

/// An unsuccessful `Fetcher` process result, where the target was not reached.
///
/// Note that the caller can still decide if the process was a success based on
/// the [`FetchResults`].
pub struct TargetMissed {
    progress: Progress,
    target: Target,
    results: FetchResults,
    required: usize,
    missed_nodes: BTreeSet<NodeId>,
}

impl TargetMissed {
    /// Get the final [`Progress`] of the fetcher result.
    pub fn progress(&self) -> Progress {
        self.progress
    }

    /// Get the [`Target`] that was trying to be reached.
    pub fn target(&self) -> &Target {
        &self.target
    }

    /// Get the final [`FetchResults`] of the fetcher result.
    pub fn fetch_results(&self) -> &FetchResults {
        &self.results
    }

    /// Get the set of nodes that were missed when attempting to fetch.
    pub fn missed_nodes(&self) -> &BTreeSet<NodeId> {
        &self.missed_nodes
    }

    /// Get the number of nodes that were required to reach the replication
    /// target.
    pub fn required_nodes(&self) -> usize {
        self.required
    }
}

/// The result of a [`Fetcher`] process.
pub enum FetcherResult {
    /// The target was reached and the process is considered a success.
    TargetReached(Success),
    /// The replication factor could not be reached at all, neither minimum nor
    /// maximum, and so this fetch should be considered an error.
    TargetError(TargetMissed),
}

impl FetcherResult {
    /// Get the final [`Progress`] of the fetcher result.
    pub fn progress(&self) -> Progress {
        match self {
            FetcherResult::TargetReached(s) => s.progress(),
            FetcherResult::TargetError(f) => f.progress(),
        }
    }

    fn target_reached(
        outcome: SuccessfulOutcome,
        progress: Progress,
        results: FetchResults,
    ) -> Self {
        Self::TargetReached(Success {
            outcome,
            progress,
            results,
        })
    }

    fn target_error(
        progress: Progress,
        target: Target,
        results: FetchResults,
        missing: BTreeSet<NodeId>,
    ) -> Self {
        let required = target
            .replicas
            .lower_bound()
            .saturating_sub(progress.succeeded);
        Self::TargetError(TargetMissed {
            progress,
            target,
            results,
            missed_nodes: missing,
            required,
        })
    }
}

/// Configuration of the [`Fetcher`].
pub struct FetcherConfig {
    /// The set of seeds that are expected to replicate the repository.
    seeds: BTreeSet<NodeId>,

    /// The number of replicas to reach for the [`Fetcher`].
    replicas: ReplicationFactor,

    /// The candidate nodes that the node will attempt to fetch from.
    candidates: VecDeque<Candidate>,

    /// The identity of the local node, to ensure that it is never emitted for
    /// connecting/fetching.
    local_node: NodeId,
}

impl FetcherConfig {
    /// Setup a private network `FetcherConfig`, populating the
    /// [`FetcherConfig`]'s seeds with the allowed set from the
    /// [`PrivateNetwork`]. It is recommended that
    /// [`FetcherConfig::with_candidates`] is not used to extend the candidate
    /// set.
    ///
    /// `replicas` is the target number of seeds the [`Fetcher`] should reach
    /// before stopping.
    ///
    /// `local_node` is the [`NodeId`] of the local node, to ensure it is
    /// excluded from the [`Fetcher`] process.
    pub fn private(
        private: PrivateNetwork,
        replicas: ReplicationFactor,
        local_node: NodeId,
    ) -> Self {
        let candidates = private
            .allowed
            .clone()
            .into_iter()
            .filter(|node| *node != local_node)
            .map(Candidate::new)
            .collect::<VecDeque<_>>();
        Self {
            seeds: private.allowed,
            replicas,
            candidates,
            local_node,
        }
    }

    /// `seeds` is the target set of preferred seeds that [`Fetcher`] should
    /// attempt to fetch from. These are the initial set of candidates nodes –
    /// to add more use [`FetcherConfig::with_candidates`].
    ///
    /// `replicas` is the target number of seeds the [`Fetcher`] should reach
    /// before stopping.
    ///
    /// `local_node` is the [`NodeId`] of the local node, to ensure it is
    /// excluded from the [`Fetcher`] process.
    pub fn public(
        seeds: BTreeSet<NodeId>,
        replicas: ReplicationFactor,
        local_node: NodeId,
    ) -> Self {
        let candidates = seeds
            .clone()
            .into_iter()
            .filter(|node| *node != local_node)
            .map(Candidate::new)
            .collect::<VecDeque<_>>();
        Self {
            seeds,
            replicas,
            candidates,
            local_node,
        }
    }

    /// Extend the set of candidate nodes to attempt to fetch from.
    pub fn with_candidates(mut self, extra: impl IntoIterator<Item = Candidate>) -> Self {
        self.candidates
            .extend(extra.into_iter().filter(|c| c.nid() != self.local_node));
        self
    }
}

/// A candidate node that can be returned by [`Fetcher::next_node`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Candidate(NodeId);

impl Candidate {
    pub fn new(node: NodeId) -> Self {
        Self(node)
    }
}

impl Candidate {
    fn nid(&self) -> NodeId {
        self.0
    }
}

/// A node that is marked as connected by calling [`Fetcher::connected`].
#[derive(Debug)]
struct Connected {
    node: NodeId,
    addr: Address,
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use crate::test::arbitrary;

    use super::*;

    #[test]
    fn all_nodes_are_candidates() {
        let local = arbitrary::gen::<NodeId>(0);
        let replicas = ReplicationFactor::default();
        let seeds = arbitrary::set::<NodeId>(3..=6)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let extra_candidates = arbitrary::vec::<NodeId>(3);
        let config = FetcherConfig::public(seeds.clone(), replicas, local)
            .with_candidates(extra_candidates.clone().into_iter().map(Candidate::new));

        let mut fetcher = Fetcher::new(config).expect("fetcher should be constructed correctly");
        let mut result = Vec::with_capacity(seeds.len() + extra_candidates.len());
        let expected = seeds
            .into_iter()
            .chain(extra_candidates)
            .collect::<Vec<_>>();

        while let Some(node) = fetcher.next_node() {
            result.push(node);
        }

        // Check that there is no node for fetching, since we have not marked
        // any as connected
        assert!(fetcher.next_fetch().is_none());

        assert_eq!(result, expected);
    }

    #[test]
    fn ignores_duplicates_and_local_node() {
        let local = arbitrary::gen::<NodeId>(0);
        let replicas = ReplicationFactor::default();
        let bob = arbitrary::gen::<NodeId>(1);
        let eve = arbitrary::gen::<NodeId>(2);
        let seeds = [bob].into_iter().collect::<BTreeSet<_>>();
        let extra_candidates = vec![bob, local, eve];
        let config = FetcherConfig::public(seeds.clone(), replicas, local)
            .with_candidates(extra_candidates.clone().into_iter().map(Candidate::new));

        let mut fetcher = Fetcher::new(config).expect("fetcher should be constructed correctly");
        let mut result = Vec::with_capacity(seeds.len() + extra_candidates.len());
        let expected = vec![bob, eve];

        while let Some(node) = fetcher.next_node() {
            fetcher.fetch_failed(node, "could not connect");
            result.push(node);
        }

        assert_eq!(result, expected);
    }

    #[test]
    fn all_nodes_are_fetchable() {
        let local = arbitrary::gen::<NodeId>(0);
        let replicas = ReplicationFactor::default();
        let seeds = arbitrary::set::<NodeId>(3..=6)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let extra_candidates = arbitrary::vec::<NodeId>(3);
        let config = FetcherConfig::public(seeds.clone(), replicas, local)
            .with_candidates(extra_candidates.clone().into_iter().map(Candidate::new));

        let mut fetcher = Fetcher::new(config).expect("fetcher should be constructed correctly");
        let mut result = Vec::with_capacity(seeds.len() + extra_candidates.len());
        let expected = seeds
            .into_iter()
            .chain(extra_candidates)
            .collect::<Vec<_>>();

        while let Some(node) = fetcher.next_node() {
            fetcher.connected(node, arbitrary::gen::<Address>(0));
        }

        while let Some((node, _)) = fetcher.next_fetch() {
            result.push(node);
        }

        assert_eq!(result, expected);
    }

    #[test]
    fn reaches_target_of_preferred_seeds() {
        let local = arbitrary::gen::<NodeId>(0);
        let replicas = ReplicationFactor::default();
        let seeds = arbitrary::set::<NodeId>(3..=3)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let extra_candidates = arbitrary::vec::<NodeId>(3);
        let config = FetcherConfig::public(seeds.clone(), replicas, local)
            .with_candidates(extra_candidates.clone().into_iter().map(Candidate::new));

        let mut fetcher = Fetcher::new(config).expect("fetcher should be constructed correctly");
        let mut result = Vec::with_capacity(seeds.len());
        let expected = seeds.into_iter().collect::<Vec<_>>();

        while let Some(node) = fetcher.next_node() {
            fetcher.connected(node, arbitrary::gen::<Address>(0));

            if let Some((node, _)) = fetcher.next_fetch() {
                match fetcher.fetch_complete(
                    node,
                    FetchResult::Success {
                        updated: vec![],
                        namespaces: HashSet::new(),
                        clone: false,
                    },
                ) {
                    ControlFlow::Continue(_) => result.push(node),
                    ControlFlow::Break(success) => {
                        assert_eq!(
                            *success.outcome(),
                            SuccessfulOutcome::PreferredNodes { preferred: 3 }
                        );
                        result.push(node);
                        break;
                    }
                }
            }
        }
        assert_eq!(result, expected);
    }

    #[test]
    fn reaches_target_of_replicas() {
        let local = arbitrary::gen::<NodeId>(0);
        let replicas = ReplicationFactor::must_reach(3);
        let seeds = arbitrary::set::<NodeId>(3..=3)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let extra_candidates = arbitrary::vec::<NodeId>(3);
        let config = FetcherConfig::public(seeds.clone(), replicas, local)
            .with_candidates(extra_candidates.clone().into_iter().map(Candidate::new));

        let mut fetcher = Fetcher::new(config).expect("fetcher should be constructed correctly");
        let mut result = Vec::with_capacity(extra_candidates.len());
        let expected = extra_candidates
            .clone()
            .into_iter()
            .take(replicas.lower_bound())
            .collect::<Vec<_>>();

        while let Some(node) = fetcher.next_node() {
            fetcher.connected(node, arbitrary::gen::<Address>(0));

            if let Some((node, _)) = fetcher.next_fetch() {
                if seeds.contains(&node) {
                    fetcher.fetch_failed(node, "failed fetch");
                    continue;
                }
                match fetcher.fetch_complete(
                    node,
                    FetchResult::Success {
                        updated: vec![],
                        namespaces: HashSet::new(),
                        clone: false,
                    },
                ) {
                    ControlFlow::Continue(_) => result.push(node),
                    ControlFlow::Break(success) => {
                        assert_eq!(
                            *success.outcome(),
                            SuccessfulOutcome::MinReplicas { succeeded: 3 }
                        );
                        result.push(node);
                        break;
                    }
                }
            }
        }
        assert_eq!(result, expected);
    }

    #[test]
    fn reaches_target_of_max_replicas() {
        let local = arbitrary::gen::<NodeId>(0);
        let replicas = ReplicationFactor::range(1, 3);
        let candidates = arbitrary::set::<NodeId>(3..=3);
        let seeds = candidates.iter().take(3).copied().collect::<BTreeSet<_>>();
        let extra_candidates = candidates.into_iter().skip(3).collect::<Vec<_>>();
        let config = FetcherConfig::public(seeds.clone(), replicas, local)
            .with_candidates(extra_candidates.clone().into_iter().map(Candidate::new));

        let mut fetcher = Fetcher::new(config).expect("fetcher should be constructed correctly");
        let mut result = Vec::with_capacity(extra_candidates.len());
        let expected = extra_candidates
            .clone()
            .into_iter()
            .take(replicas.upper_bound().expect("replicas must have max"))
            .collect::<Vec<_>>();

        while let Some(node) = fetcher.next_node() {
            fetcher.connected(node, arbitrary::gen::<Address>(0));

            if let Some((node, _)) = fetcher.next_fetch() {
                if seeds.contains(&node) {
                    fetcher.fetch_failed(node, "could not connect");
                    continue;
                }
                match fetcher.fetch_complete(
                    node,
                    FetchResult::Success {
                        updated: vec![],
                        namespaces: HashSet::new(),
                        clone: false,
                    },
                ) {
                    ControlFlow::Continue(_) => result.push(node),
                    ControlFlow::Break(success) => {
                        assert_eq!(
                            *success.outcome(),
                            SuccessfulOutcome::MaxReplicas {
                                succeeded: 3,
                                min: 1,
                                max: 3
                            }
                        );
                        result.push(node);
                        break;
                    }
                }
            }
        }
        assert_eq!(
            result,
            expected,
            "expected {} seed(s), found {}",
            expected.len(),
            result.len(),
        );
    }

    #[test]
    fn preferred_seeds_target_returned_over_replicas() {
        let local = arbitrary::gen::<NodeId>(0);
        let replicas = ReplicationFactor::range(1, 3);
        let candidates = arbitrary::set::<NodeId>(3..=3);
        let seeds = candidates.into_iter().collect::<BTreeSet<_>>();
        let config = FetcherConfig::public(seeds.clone(), replicas, local);

        let mut fetcher = Fetcher::new(config).expect("fetcher should be constructed correctly");
        let mut result = Vec::with_capacity(seeds.len());

        while let Some(node) = fetcher.next_node() {
            fetcher.connected(node, arbitrary::gen::<Address>(0));

            if let Some((node, _)) = fetcher.next_fetch() {
                match fetcher.fetch_complete(
                    node,
                    FetchResult::Success {
                        updated: vec![],
                        namespaces: HashSet::new(),
                        clone: false,
                    },
                ) {
                    ControlFlow::Continue(_) => result.push(node),
                    ControlFlow::Break(success) => {
                        assert_eq!(
                            *success.outcome(),
                            SuccessfulOutcome::PreferredNodes { preferred: 3 }
                        );
                        result.push(node);
                        break;
                    }
                }
            }
        }
        assert_eq!(result, seeds.into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn could_not_reach_target() {
        let local = arbitrary::gen::<NodeId>(0);
        let replicas = ReplicationFactor::must_reach(4);
        let candidates = arbitrary::set::<NodeId>(3..=3);
        let seeds = candidates.into_iter().collect::<BTreeSet<_>>();
        let config = FetcherConfig::public(seeds.clone(), replicas, local);

        let mut fetcher = Fetcher::new(config).expect("fetcher should be constructed correctly");

        while let Some(node) = fetcher.next_node() {
            fetcher.connected(node, arbitrary::gen::<Address>(0));

            if let Some((node, _)) = fetcher.next_fetch() {
                fetcher.fetch_failed(node, "could not connect");
            }
        }
        let result = fetcher.finish();
        assert!(matches!(result, FetcherResult::TargetError(_)));
    }
}
