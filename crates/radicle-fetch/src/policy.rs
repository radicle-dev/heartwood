use std::collections::HashSet;

use radicle::crypto::PublicKey;
use radicle::node::policy::config::Config;
use radicle::node::policy::store::Read;
use radicle::prelude::RepoId;

pub use radicle::node::policy::{Policy, Scope, SeedingPolicy};

#[derive(Clone, Debug)]
pub enum Allowed {
    All,
    Followed { remotes: HashSet<PublicKey> },
}

impl Allowed {
    pub fn from_config(rid: RepoId, config: &Config<Read>) -> Result<Self, error::Policy> {
        let entry = config
            .seed_policy(&rid)
            .map_err(|err| error::Policy::FailedPolicy { rid, err })?;
        match entry.policy {
            SeedingPolicy::Block => {
                log::error!(target: "fetch", "Attempted to fetch non-seeded repo {rid}");
                Err(error::Policy::BlockedPolicy { rid })
            }
            SeedingPolicy::Allow { scope: Scope::All } => Ok(Self::All),
            SeedingPolicy::Allow {
                scope: Scope::Followed,
            } => {
                let nodes = config
                    .follow_policies()
                    .map_err(|err| error::Policy::FailedNodes { rid, err })?;

                let mut followed = HashSet::new();

                for node in nodes {
                    let node = match node {
                        Ok(policy) => policy,
                        Err(err) => {
                            log::error!(target: "fetch", "Failed to read follow policy for {rid}: {err}");
                            continue;
                        }
                    };

                    if node.policy == Policy::Allow {
                        followed.insert(node.nid);
                    }
                }

                Ok(Allowed::Followed { remotes: followed })
            }
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
        let mut blocked = HashSet::new();

        for entry in config.follow_policies()? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    log::error!(target: "fetch", "Failed to read follow policy: {err}");
                    continue;
                }
            };

            if entry.policy == Policy::Block {
                blocked.insert(entry.nid);
            }
        }

        Ok(BlockList(blocked))
    }
}

pub mod error {
    use radicle::node::policy;
    use radicle::prelude::RepoId;
    use radicle::storage;
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[error(transparent)]
    pub struct Blocked(#[from] policy::config::Error);

    #[derive(Debug, Error)]
    pub enum Policy {
        #[error("failed to find policy for {rid}")]
        FailedPolicy {
            rid: RepoId,
            #[source]
            err: policy::store::Error,
        },
        #[error("cannot fetch {rid} as it is not seeded")]
        BlockedPolicy { rid: RepoId },
        #[error("failed to get followed nodes for {rid}")]
        FailedNodes {
            rid: RepoId,
            #[source]
            err: policy::store::Error,
        },

        #[error(transparent)]
        Storage(#[from] storage::Error),

        #[error(transparent)]
        Git(#[from] radicle::git::raw::Error),

        #[error(transparent)]
        Refs(#[from] storage::refs::Error),
    }
}
