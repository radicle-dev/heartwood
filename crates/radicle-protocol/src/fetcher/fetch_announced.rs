use std::collections::HashSet;
use std::marker::PhantomData;
use std::ops::ControlFlow;
use std::time;

use nonempty::NonEmpty;
use radicle::git;
use radicle::node::{policy, NodeId};
use radicle::prelude::RepoId;
use radicle::storage::refs::RefsAt;

use super::{FetchResult, FetcherState};

/// Provide a way to fetch an announcement from a node.
pub struct FetchAnnounced<'a, P, R, N> {
    policies: &'a P,
    refs: &'a R,
    namespaces: &'a N,
    local: NodeId,
}

/// An announcement for advertised signed references that should be marked for
/// fetching.
pub struct Announced {
    /// The repository that should be fetched.
    pub rid: RepoId,
    /// The node that advertised the signed references.
    pub from: NodeId,
    /// The signed references that were advertised.
    pub announced: NonEmpty<RefsAt>,
}

impl<'a, P, R, N> FetchAnnounced<'a, P, R, N>
where
    P: SeedingPolicies,
    R: SingedRefsOf,
    N: Following,
{
    /// Provide the local [`NodeId`] for ensuring that the it is not included in
    /// the fetch, and the [`policy::Scope`] for the repository to be fetched.
    pub fn new(local: NodeId, policies: &'a P, refs: &'a R, namespaces: &'a N) -> Self {
        Self {
            policies,
            refs,
            namespaces,
            local,
        }
    }

    /// Check the [`Announced`] references for signed references that the local
    /// node wants.
    ///
    /// A [`FetchResult`] is only returned if the announcement contains any
    /// wanted signed references.
    pub fn fetch(
        self,
        fetcher: &FetcherState,
        announced: Announced,
        timeout: time::Duration,
    ) -> Result<Option<FetchResult>, FetchAnnouncedError> {
        let Announced {
            rid,
            from,
            announced,
        } = announced;

        let scope = match self.policies.seed_policy(&rid)? {
            policy::SeedingPolicy::Allow { scope } => scope,
            policy::SeedingPolicy::Block => return Err(FetchAnnouncedError::Blocked { rid }),
        };

        let mut wants = WantsFromNode::new(self.local);
        for theirs in announced {
            let ours = self.refs.signed_refs_of(&rid, &theirs.remote)?;
            wants.want(theirs, ours);
        }
        let wants = match wants.ready_or_get_namespaces(scope) {
            ControlFlow::Continue(status) => {
                let namespaces = self.namespaces.followed_nodes(&rid)?;
                status.filter_following(namespaces).into_wants_haves()
            }
            ControlFlow::Break(status) => status.into_wants_haves(),
        };
        Ok(wants.map(|Wants { wants, .. }| fetcher.fetch(rid, from, wants.into(), timeout)))
    }
}

/// Build the set of "wants" that we would like to fetch from a node.
struct WantsFromNode<T> {
    /// The local [`NodeId`] is used for ensuring that it is not added to the
    /// set of wants, i.e. we do not want to fetch our own references.
    local: NodeId,
    /// The set of wants that we are building.
    wants: WantsBuilder,
    /// Marks state transitions for building the set of wants.
    _marker: PhantomData<T>,
}

/// Marker for the initial state of the wants building process.
struct Initial;

/// Marker for the namespaces state.
struct FilterFollowing;

/// Marker for the ready state.
struct Ready;

impl WantsFromNode<Initial> {
    /// Initialize the wants building process.
    fn new(local: NodeId) -> Self {
        Self {
            local,
            wants: WantsBuilder::default(),
            _marker: PhantomData,
        }
    }

    /// Check if the given [`RefsAt`] is wanted by us.
    fn want(&mut self, theirs: RefsAt, ours: Option<git::Oid>) {
        if theirs.remote != self.local {
            self.wants.want(theirs, ours);
        }
    }

    /// Transition to one of two states:
    ///   1. We are ready to finalize the wants.
    ///   2. We need require providing [`storage::Namespaces`] to filter by.
    fn ready_or_get_namespaces(
        self,
        scope: policy::Scope,
    ) -> ControlFlow<WantsFromNode<Ready>, WantsFromNode<FilterFollowing>> {
        if self.wants.wants.is_empty() {
            return ControlFlow::Break(WantsFromNode {
                local: self.local,
                wants: self.wants,
                _marker: PhantomData,
            });
        }
        match scope {
            policy::Scope::Followed => ControlFlow::Continue(WantsFromNode {
                local: self.local,
                wants: self.wants,
                _marker: PhantomData,
            }),
            policy::Scope::All => ControlFlow::Break(WantsFromNode {
                local: self.local,
                wants: self.wants,
                _marker: PhantomData,
            }),
        }
    }
}

impl WantsFromNode<FilterFollowing> {
    /// Optionally filter by a set of nodes that are being followed by the local
    /// node.
    fn filter_following(mut self, following: Option<HashSet<NodeId>>) -> WantsFromNode<Ready> {
        if let Some(followed) = following {
            self.wants.wants.retain(|at| followed.contains(&at.remote));
        }
        WantsFromNode {
            local: self.local,
            wants: self.wants,
            _marker: PhantomData,
        }
    }
}

impl WantsFromNode<Ready> {
    /// Build the [`Wants`], ensuring that they are not empty.
    fn into_wants_haves(self) -> Option<Wants> {
        self.wants.build()
    }
}

/// The non-empty set of wants.
struct Wants {
    wants: NonEmpty<RefsAt>,
}

/// Track wants we require from the node.
#[derive(Default)]
struct WantsBuilder {
    /// The set of signed references for a given advertised remote.
    ///
    /// We *want* the signed references if a new one is advertised by the
    /// remote, or we have never seen it.
    wants: Vec<RefsAt>,
}

impl WantsBuilder {
    fn want(&mut self, theirs: RefsAt, ours: Option<git::Oid>) {
        match ours {
            Some(ours) => {
                if theirs.at != ours {
                    self.wants.push(theirs);
                }
            }
            None => {
                self.wants.push(theirs);
            }
        }
    }

    fn build(self) -> Option<Wants> {
        NonEmpty::from_vec(self.wants).map(|wants| Wants { wants })
    }
}

/// Error occurred when processing an announcement for fetching.
#[derive(Debug, thiserror::Error)]
pub enum FetchAnnouncedError {
    /// The repository is marked as blocked by the configured seeding policy.
    #[error("the repository {rid} is blocked for fetching")]
    Blocked { rid: RepoId },
    #[error(transparent)]
    Seeding(#[from] SeedingPolicyError),
    /// An error occurred when attempting to retrieve the signed references value
    /// for a given repository and node.
    #[error(transparent)]
    SingedRefs(#[from] SignedRefsError),
    #[error(transparent)]
    /// An error occurred when attempting to retrieve the namespaces value for a
    /// given repository.
    Followed(#[from] FollowedNodesError),
}

#[derive(Debug, thiserror::Error)]
pub enum SignedRefsError {
    #[error("failed to get signed references commit of {node} in {rid}")]
    Other {
        rid: RepoId,
        node: NodeId,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

pub trait SingedRefsOf {
    /// Retrieve the known commit for the signed references value of the `node`
    /// within the repository identified by `rid`.
    ///
    /// If the commit is it not known, then `None` should be returned.
    ///
    /// **Note**: this can be cached value of the commit, rather than the Git
    /// repository itself.
    fn signed_refs_of(
        &self,
        rid: &RepoId,
        node: &NodeId,
    ) -> Result<Option<git::Oid>, SignedRefsError>;
}

#[derive(Debug, thiserror::Error)]
pub enum FollowedNodesError {
    #[error("failed to get namespaces for {rid} due to: {source}")]
    Other {
        rid: RepoId,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

pub trait Following {
    /// Retrieve the nodes being followed for the repository identified by
    /// `rid`.
    ///
    /// May return `None` if no filtering is required.
    fn followed_nodes(&self, rid: &RepoId) -> Result<Option<HashSet<NodeId>>, FollowedNodesError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SeedingPolicyError {
    #[error("failed to get seeding policy for {rid} due to: {source}")]
    Other {
        rid: RepoId,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

pub trait SeedingPolicies {
    /// Retrieve the [`policy::SeedingPolicy`] for the repository identified by
    /// `rid`.
    fn seed_policy(&self, rid: &RepoId) -> Result<policy::SeedingPolicy, SeedingPolicyError>;
}
