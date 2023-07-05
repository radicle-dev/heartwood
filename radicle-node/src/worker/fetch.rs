pub mod error;

use std::collections::HashSet;

use radicle::crypto::PublicKey;
use radicle::git::UserInfo;
use radicle::prelude::Id;
use radicle::storage::git::Repository;
use radicle::storage::{ReadStorage as _, RefUpdate, WriteRepository as _, WriteStorage as _};
use radicle::Storage;
use radicle_fetch::{BlockList, FetchLimit, Tracked};

use super::channels::ChannelsFlush;

#[derive(Debug, Default)]
pub struct FetchResult {
    /// The set of updates references.
    pub updated: Vec<RefUpdate>,
    /// The set of remote namespaces that were updated.
    pub namespaces: HashSet<PublicKey>,
}

pub enum Handle {
    Clone {
        handle: radicle_fetch::Handle<ChannelsFlush>,
    },
    Pull {
        handle: radicle_fetch::Handle<ChannelsFlush>,
    },
}

impl Handle {
    pub fn new(
        rid: Id,
        local: PublicKey,
        info: UserInfo,
        storage: &Storage,
        tracked: Tracked,
        blocked: BlockList,
        channels: ChannelsFlush,
    ) -> Result<Self, error::Handle> {
        let exists = storage.contains(&rid)?;
        if exists {
            let repo = storage.repository(rid)?;
            let handle = radicle_fetch::Handle::new(local, repo, tracked, blocked, channels)?;
            Ok(Handle::Pull { handle })
        } else {
            let repo = storage.create(rid)?;
            repo.set_user(&info)?;
            let handle = radicle_fetch::Handle::new(local, repo, tracked, blocked, channels)?;
            Ok(Handle::Clone { handle })
        }
    }

    pub fn fetch(
        mut self,
        rid: Id,
        storage: &Storage,
        limit: FetchLimit,
        remote: PublicKey,
    ) -> Result<FetchResult, error::Fetch> {
        let result = match &mut self {
            Self::Clone { handle } => {
                log::debug!(target: "worker", "{} cloning from {remote}", handle.local());
                match radicle_fetch::clone(handle, limit, remote) {
                    Ok(result) => result,
                    Err(e) => {
                        // N.b. the clone failed so we remove the
                        // repository from the storage
                        storage.remove(rid)?;
                        return Err(e.into());
                    }
                }
            }
            Self::Pull { handle } => {
                log::debug!(target: "worker", "{} pulling from {remote}", handle.local());
                radicle_fetch::pull(handle, limit, remote)?
            }
        };

        for rejected in result.rejected() {
            log::warn!(target: "worker", "Rejected update for {}", rejected.refname())
        }

        for warn in result.warnings() {
            log::warn!(target: "worker", "Validation error: {}", warn);
        }

        match result {
            radicle_fetch::FetchResult::Failed { failures, .. } => {
                for fail in failures.iter() {
                    log::error!(target: "worker", "Validation error: {}", fail);
                }
                Err(error::Fetch::Validation)
            }
            radicle_fetch::FetchResult::Success {
                applied, remotes, ..
            } => {
                self.repository_mut().set_head()?;
                self.repository_mut().set_identity_head()?;

                Ok(FetchResult {
                    updated: applied.updated,
                    namespaces: remotes.into_iter().collect(),
                })
            }
        }
    }

    fn repository_mut(&mut self) -> &mut Repository {
        match self {
            Self::Clone { handle } => handle.repository_mut(),
            Self::Pull { handle } => handle.repository_mut(),
        }
    }
}
