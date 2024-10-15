use std::sync::atomic::{self, AtomicBool};
use std::sync::Arc;

use bstr::BString;
use radicle::crypto::PublicKey;
use radicle::git::Oid;
use radicle::identity::{DocError, Doc};
use radicle::storage::git::Repository;
use radicle::storage::ReadRepository;

use crate::policy::{Allowed, BlockList};
use crate::transport::{ConnectionStream, Transport};

/// The handle used for pulling or cloning changes from a remote peer.
pub struct Handle<S> {
    pub(crate) local: PublicKey,
    pub(crate) repo: Repository,
    pub(crate) allowed: Allowed,
    pub(crate) transport: Transport<S>,
    /// The set of keys we will ignore when fetching from a
    /// remote. This set can be constructed using the tracking
    /// `config`'s blocked node entries.
    ///
    /// Note that it's important to ignore the local peer's
    /// key in [`crate::pull`], however, we choose to allow the local
    /// peer's key in [`crate::clone`].
    pub(crate) blocked: BlockList,
    // Signals to the pack writer to interrupt the process
    pub(crate) interrupt: Arc<AtomicBool>,
}

impl<S> Handle<S> {
    pub fn new(
        local: PublicKey,
        repo: Repository,
        follow: Allowed,
        blocked: BlockList,
        connection: S,
    ) -> Result<Self, error::Init>
    where
        S: ConnectionStream,
    {
        let git_dir = repo.backend.path().to_path_buf();
        let transport = Transport::new(git_dir, BString::from(repo.id.canonical()), connection);

        Ok(Self {
            local,
            repo,
            allowed: follow,
            transport,
            blocked,
            interrupt: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn is_blocked(&self, key: &PublicKey) -> bool {
        self.blocked.is_blocked(key)
    }

    pub fn repository(&self) -> &Repository {
        &self.repo
    }

    pub fn repository_mut(&mut self) -> &mut Repository {
        &mut self.repo
    }

    pub fn local(&self) -> &PublicKey {
        &self.local
    }

    pub fn interrupt_pack_writer(&mut self) {
        self.interrupt.store(true, atomic::Ordering::Relaxed);
    }

    pub fn verified(&self, head: Oid) -> Result<Doc, DocError> {
        Ok(self.repo.identity_doc_at(head)?.doc)
    }

    pub fn allowed(&self) -> Allowed {
        self.allowed.clone()
    }
}

pub mod error {
    use std::io;

    use radicle::node::policy;
    use radicle::prelude::RepoId;
    use radicle::{git, storage};
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum Init {
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error(transparent)]
        Tracking(#[from] policy::config::Error),
    }

    #[derive(Debug, Error)]
    pub enum Tracking {
        #[error("failed to find policy for {rid}")]
        FailedPolicy {
            rid: RepoId,
            #[source]
            err: policy::store::Error,
        },
        #[error("cannot fetch {rid} as it is not seeded")]
        BlockedPolicy { rid: RepoId },
        #[error("failed to get tracking nodes for {rid}")]
        FailedNodes {
            rid: RepoId,
            #[source]
            err: policy::store::Error,
        },

        #[error(transparent)]
        Storage(#[from] storage::Error),

        #[error(transparent)]
        Git(#[from] git::raw::Error),

        #[error(transparent)]
        Refs(#[from] storage::refs::Error),
    }
}
