use core::fmt;
use std::collections::HashSet;
use std::ops;

use log::error;
use thiserror::Error;

use crate::crypto::PublicKey;
use crate::node::NodeId;
use crate::prelude::RepoId;
use crate::storage::{Namespaces, ReadRepository as _, ReadStorage, RepositoryError};

pub use crate::node::policy::store;
pub use crate::node::policy::store::Error;
pub use crate::node::policy::store::Store;
pub use crate::node::policy::{Alias, FollowPolicy, Policy, Scope, SeedPolicy, SeedingPolicy};

#[derive(Debug, Error)]
pub enum NamespacesError {
    #[error("failed to find policy for {rid}")]
    FailedPolicy {
        rid: RepoId,
        #[source]
        err: Error,
    },
    #[error("cannot fetch {rid} as it is not seeded")]
    BlockedPolicy { rid: RepoId },
    #[error("failed to get node policies for {rid}")]
    FailedNodes {
        rid: RepoId,
        #[source]
        err: Error,
    },
    #[error("failed to get delegates for {rid}")]
    FailedDelegates {
        rid: RepoId,
        #[source]
        err: RepositoryError,
    },
    #[error(transparent)]
    Git(#[from] crate::git::raw::Error),
    #[error("could not find any followed nodes for {rid}")]
    NoFollowed { rid: RepoId },
}

/// Policies configuration.
pub struct Config<T> {
    /// Default policy, if a policy for a specific node or repository was not found.
    policy: SeedingPolicy,
    /// Underlying configuration store.
    store: Store<T>,
}

// N.b. deriving `Debug` will require `T: Debug` so we manually
// implement it here.
impl<T> fmt::Debug for Config<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("policy", &self.policy)
            .field("store", &self.store)
            .finish()
    }
}

impl<T> Config<T> {
    /// Create a new policy configuration.
    pub fn new(policy: SeedingPolicy, store: Store<T>) -> Self {
        Self { policy, store }
    }

    /// Check if a repository is seeded.
    pub fn is_seeding(&self, rid: &RepoId) -> Result<bool, Error> {
        self.seed_policy(rid).map(|entry| entry.policy.is_allow())
    }

    /// Get a repository's seeding information.
    /// Returns the default policy if the repo isn't found.
    pub fn seed_policy(&self, rid: &RepoId) -> Result<SeedPolicy, Error> {
        Ok(self.store.seed_policy(rid)?.unwrap_or(SeedPolicy {
            rid: *rid,
            policy: self.policy,
        }))
    }

    pub fn namespaces_for<S>(
        &self,
        storage: &S,
        rid: &RepoId,
    ) -> Result<Namespaces, NamespacesError>
    where
        S: ReadStorage,
    {
        use NamespacesError::*;

        let entry = self
            .seed_policy(rid)
            .map_err(|err| FailedPolicy { rid: *rid, err })?;
        match entry.policy {
            SeedingPolicy::Block => {
                error!(target: "service", "Attempted to fetch untracked repo {rid}");
                Err(NamespacesError::BlockedPolicy { rid: *rid })
            }
            SeedingPolicy::Allow { scope: Scope::All } => Ok(Namespaces::All),
            SeedingPolicy::Allow {
                scope: Scope::Followed,
            } => {
                let nodes = self
                    .follow_policies()
                    .map_err(|err| FailedNodes { rid: *rid, err })?;
                let mut followed: HashSet<_> = nodes
                    .filter_map(|node| (node.policy == Policy::Allow).then_some(node.nid))
                    .collect();

                if let Ok(repo) = storage.repository(*rid) {
                    // TODO(finto): need to have something that resolves the
                    // node id associated with each delegate
                    let delegates = repo
                        .delegates()
                        .map_err(|err| FailedDelegates { rid: *rid, err })?
                        .map(NodeId::from);
                    followed.extend(delegates);
                };
                if followed.is_empty() {
                    // Nb. returning All here because the
                    // fetching logic will correctly determine
                    // followed and delegate remotes.
                    Ok(Namespaces::All)
                } else {
                    Ok(Namespaces::Followed(followed))
                }
            }
        }
    }
}

impl<T> ops::Deref for Config<T> {
    type Target = Store<T>;

    fn deref(&self) -> &Self::Target {
        &self.store
    }
}

impl<T> ops::DerefMut for Config<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.store
    }
}
