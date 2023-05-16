use std::collections::HashSet;

use radicle::crypto::PublicKey;
use radicle::node::tracking::config::Config;
use radicle::node::tracking::store::Read;
use radicle::prelude::Id;

pub use radicle::node::tracking::{Policy, Scope};

#[derive(Clone, Debug)]
pub enum Tracked {
    All,
    Trusted { remotes: HashSet<PublicKey> },
}

impl Tracked {
    pub fn from_config(rid: Id, config: &Config<Read>) -> Result<Self, error::Tracking> {
        let entry = config
            .repo_policy(&rid)
            .map_err(|err| error::Tracking::FailedPolicy { rid, err })?;
        match entry.policy {
            Policy::Block => {
                log::error!(target: "fetch", "Attempted to fetch untracked repo {rid}");
                Err(error::Tracking::BlockedPolicy { rid })
            }
            Policy::Track => match entry.scope {
                Scope::All => Ok(Self::All),
                Scope::Trusted => {
                    let nodes = config
                        .node_policies()
                        .map_err(|err| error::Tracking::FailedNodes { rid, err })?;
                    let trusted: HashSet<_> = nodes
                        .filter_map(|node| (node.policy == Policy::Track).then_some(node.id))
                        .collect();

                    Ok(Tracked::Trusted { remotes: trusted })
                }
            },
        }
    }
}

/// A set of [`PublicKey`]s to ignore when fetching from a remote.
#[derive(Clone, Debug)]
pub struct BlockList(HashSet<PublicKey>);

impl FromIterator<PublicKey> for BlockList {
    fn from_iter<T: IntoIterator<Item = PublicKey>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl Extend<PublicKey> for BlockList {
    fn extend<T: IntoIterator<Item = PublicKey>>(&mut self, iter: T) {
        self.0.extend(iter)
    }
}

impl BlockList {
    pub fn is_blocked(&self, key: &PublicKey) -> bool {
        self.0.contains(key)
    }

    pub fn from_config(config: &Config<Read>) -> Result<BlockList, error::Blocked> {
        Ok(config
            .node_policies()?
            .filter_map(|entry| (entry.policy == Policy::Block).then_some(entry.id))
            .collect())
    }
}

pub mod error {
    use radicle::node::tracking;
    use radicle::prelude::Id;
    use radicle::storage;
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[error(transparent)]
    pub struct Blocked(#[from] tracking::config::Error);

    #[derive(Debug, Error)]
    pub enum Tracking {
        #[error("failed to find tracking policy for {rid}")]
        FailedPolicy {
            rid: Id,
            #[source]
            err: tracking::store::Error,
        },
        #[error("cannot fetch {rid} as it is not tracked")]
        BlockedPolicy { rid: Id },
        #[error("failed to get tracking nodes for {rid}")]
        FailedNodes {
            rid: Id,
            #[source]
            err: tracking::store::Error,
        },

        #[error(transparent)]
        Storage(#[from] storage::Error),

        #[error(transparent)]
        Git(#[from] radicle::git::raw::Error),

        #[error(transparent)]
        Refs(#[from] storage::refs::Error),
    }
}
