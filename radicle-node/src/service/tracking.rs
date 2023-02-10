mod store;

use std::ops;
use std::str::FromStr;

use crate::prelude::Id;
use crate::service::NodeId;

pub use store::Config as Store;
pub use store::Error;

/// Node alias.
pub type Alias = String;

/// Tracking policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Policy {
    /// The resource is tracked.
    Track,
    /// The resource is blocked.
    Block,
}

/// Tracking scope of a repository tracking policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    /// Track remotes of nodes that are already tracked.
    Trusted,
    /// Track remotes of repository delegates.
    DelegatesOnly,
    /// Track all remotes.
    All,
}

impl FromStr for Scope {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "trusted" => Ok(Self::Trusted),
            "delegates-only" => Ok(Self::DelegatesOnly),
            "all" => Ok(Self::All),
            _ => Err(()),
        }
    }
}

/// Tracking configuration.
#[derive(Debug)]
pub struct Config {
    /// Default policy, if a policy for a specific node or repository was not found.
    default: Policy,
    /// Underlying configuration store.
    store: store::Config,
}

impl Config {
    /// Create a new tracking configuration.
    pub fn new(default: Policy, store: store::Config) -> Self {
        Self { default, store }
    }

    /// Check if a repository is tracked.
    pub fn is_repo_tracked(&self, id: &Id) -> Result<bool, Error> {
        if self.default == Policy::Track {
            return Ok(true);
        }
        self.store.is_repo_tracked(id)
    }

    /// Check if a node is tracked.
    pub fn is_node_tracked(&self, id: &NodeId) -> Result<bool, Error> {
        if self.default == Policy::Track {
            return Ok(true);
        }
        self.store.is_node_tracked(id)
    }

    /// Get a node's tracking information.
    pub fn node_entry(&self, id: &NodeId) -> Result<(Option<Alias>, Policy), Error> {
        if let Some(result) = self.store.node_entry(id)? {
            return Ok(result);
        }
        Ok((None, self.default))
    }

    /// Get a repository's tracking information.
    pub fn repo_entry(&self, id: &Id) -> Result<(Scope, Policy), Error> {
        if let Some(result) = self.store.repo_entry(id)? {
            return Ok(result);
        }
        Ok((Scope::All, self.default))
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
