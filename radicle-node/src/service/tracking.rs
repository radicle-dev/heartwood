mod store;

use std::ops;

use crate::prelude::Id;
use crate::service::NodeId;

pub use crate::node::tracking::{Alias, Node, Policy, Repo, Scope};

pub use store::Config as Store;
pub use store::Error;

/// Tracking configuration.
#[derive(Debug)]
pub struct Config {
    /// Default policy, if a policy for a specific node or repository was not found.
    policy: Policy,
    #[allow(dead_code)]
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
        self.repo_policy(id).map(|entry| entry == Policy::Track)
    }

    /// Check if a node is tracked.
    pub fn is_node_tracked(&self, id: &NodeId) -> Result<bool, Error> {
        self.node_policy(id).map(|entry| entry == Policy::Track)
    }

    /// Get a node's tracking information.
    /// Returns the default policy if the node isn't found.
    pub fn node_policy(&self, id: &NodeId) -> Result<Policy, Error> {
        if let Some((_, policy)) = self.store.node_entry(id)? {
            return Ok(policy);
        }
        Ok(self.policy)
    }

    /// Get a repository's tracking information.
    /// Returns the default policy if the repo isn't found.
    pub fn repo_policy(&self, id: &Id) -> Result<Policy, Error> {
        if let Some((_, policy)) = self.store.repo_entry(id)? {
            return Ok(policy);
        }
        Ok(self.policy)
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
