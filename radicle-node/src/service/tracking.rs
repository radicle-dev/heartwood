mod store;

use std::ops;

use log::{error, warn};
use nonempty::NonEmpty;
use thiserror::Error;

use radicle::crypto::PublicKey;
use radicle::identity::IdentityError;
use radicle::storage::{Namespaces, ReadRepository as _, ReadStorage};

use crate::prelude::Id;
use crate::service::NodeId;

pub use crate::node::tracking::{Alias, Node, Policy, Repo, Scope};

pub use store::Config as Store;
pub use store::Error;

#[derive(Debug, Error)]
pub enum NamespacesError {
    #[error("Failed to find tracking policy for {rid}")]
    FailedPolicy {
        rid: Id,
        #[source]
        err: Error,
    },
    #[error("The policy for {rid} is to block fetching")]
    BlockedPolicy { rid: Id },
    #[error("Failed to get tracking nodes for {rid}")]
    FailedNodes {
        rid: Id,
        #[source]
        err: Error,
    },
    #[error("Failed to get delegate nodes for {rid}")]
    FailedDelegates {
        rid: Id,
        #[source]
        err: IdentityError,
    },
    #[error("Could not find any trusted nodes for {rid}")]
    NoTrusted { rid: Id },
}

/// Tracking configuration.
#[derive(Debug)]
pub struct Config {
    /// Default policy, if a policy for a specific node or repository was not found.
    policy: Policy,
    /// Default scope, if a scope for a specific repository was not found.
    scope: Scope,
    /// Underlying configuration store.
    store: store::Config,
}

impl Config {
    /// Create a new tracking configuration.
    pub fn new(policy: Policy, scope: Scope, store: store::Config) -> Self {
        Self {
            policy,
            scope,
            store,
        }
    }

    /// Check if a repository is tracked.
    pub fn is_repo_tracked(&self, id: &Id) -> Result<bool, Error> {
        self.repo_policy(id)
            .map(|entry| entry.policy == Policy::Track)
    }

    /// Check if a node is tracked.
    pub fn is_node_tracked(&self, id: &NodeId) -> Result<bool, Error> {
        self.node_policy(id)
            .map(|entry| entry.policy == Policy::Track)
    }

    /// Get a node's tracking information.
    /// Returns the default policy if the node isn't found.
    pub fn node_policy(&self, id: &NodeId) -> Result<Node, Error> {
        Ok(self.store.node_entry(id)?.unwrap_or(Node {
            id: *id,
            alias: None,
            policy: self.policy,
        }))
    }

    /// Get a repository's tracking information.
    /// Returns the default policy if the repo isn't found.
    pub fn repo_policy(&self, id: &Id) -> Result<Repo, Error> {
        Ok(self.store.repo_entry(id)?.unwrap_or(Repo {
            id: *id,
            scope: self.scope,
            policy: self.policy,
        }))
    }

    /// Retrieve namespaces in `storage` for `rid` when in session with `remote`.
    pub fn namespaces_for<S>(
        &self,
        storage: &S,
        rid: &Id,
        remote: &NodeId,
    ) -> Result<Namespaces, NamespacesError>
    where
        S: ReadStorage,
    {
        use NamespacesError::*;

        let entry = self
            .repo_policy(rid)
            .map_err(|err| FailedPolicy { rid: *rid, err })?;
        match entry.policy {
            Policy::Block => {
                error!(target: "service", "Attempted to fetch blocked repo {rid}");
                Err(NamespacesError::BlockedPolicy { rid: *rid })
            }
            Policy::Track => match self.scope {
                Scope::All => Ok(Namespaces::All),
                Scope::Trusted => {
                    let nodes = self
                        .node_entries()
                        .map_err(|err| FailedNodes { rid: *rid, err })?;
                    let mut trusted: Vec<_> = nodes
                        .filter_map(|node| (node.policy == Policy::Track).then_some(node.id))
                        .collect();

                    if let Ok(repo) = storage.repository(*rid) {
                        let delegates = repo
                            .delegates()
                            .map_err(|err| FailedDelegates { rid: *rid, err })?
                            .map(PublicKey::from);
                        trusted.extend(delegates);
                    } else {
                        // We don't have this repo yet, we should include the `remote`
                        // in addition to `trusted`.
                        trusted.push(*remote);
                    }

                    let ns = NonEmpty::from_vec(trusted).map(Namespaces::Many);
                    ns.ok_or_else(|| {
                        warn!(target: "service", "Attempted to fetch repo {rid} with no trusted peers");
                        NoTrusted { rid: *rid }
                    })
                }
            },
        }
    }
}

impl ops::Deref for Config {
    type Target = store::Config;

    fn deref(&self) -> &Self::Target {
        &self.store
    }
}

impl ops::DerefMut for Config {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.store
    }
}
