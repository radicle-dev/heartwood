#![allow(clippy::too_many_arguments)]
pub mod fetch;

use std::io;

use radicle::identity::RepoId;
use radicle::node::Event;
use radicle::prelude::NodeId;
use radicle::storage::refs::RefsAt;

use radicle::node::events::Emitter;

/// Error returned by fetch.
#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error("the 'git fetch' command failed with exit code '{code}'")]
    CommandFailed { code: i32 },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Fetch(#[from] fetch::error::Fetch),
    #[error(transparent)]
    Handle(#[from] fetch::error::Handle),
    #[error(transparent)]
    Storage(#[from] radicle::storage::Error),
    #[error(transparent)]
    PolicyStore(#[from] radicle::node::policy::store::Error),
    #[error(transparent)]
    Policy(#[from] radicle_fetch::policy::error::Policy),
    #[error(transparent)]
    Blocked(#[from] radicle_fetch::policy::error::Blocked),
}

impl FetchError {
    /// Check if it's a timeout error.
    pub fn is_timeout(&self) -> bool {
        matches!(self, FetchError::Io(e) if e.kind() == io::ErrorKind::TimedOut)
    }
}

/// Error returned by fetch responder.
#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error("error parsing git command packet-line: {0}")]
    PacketLine(io::Error),
    #[error("error while performing git upload-pack: {0}")]
    UploadPack(io::Error),
    #[error(transparent)]
    Authorization(#[from] AuthorizationError),
}

impl UploadError {
    /// Check if it's an end-of-file error.
    pub fn is_eof(&self) -> bool {
        matches!(self, UploadError::UploadPack(e) if e.kind() == io::ErrorKind::UnexpectedEof)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum AuthorizationError {
    #[error("{0} is not authorized to fetch {1}")]
    Unauthorized(NodeId, RepoId),
    #[error(transparent)]
    PolicyStore(#[from] radicle::node::policy::store::Error),
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
}

/// Fetch job sent to worker thread.
#[derive(Debug, Clone)]
pub enum FetchRequest {
    /// Client is initiating a fetch for the repository identified by
    /// `rid` from the peer identified by `remote`.
    Initiator {
        /// Repo to fetch.
        rid: RepoId,
        /// Remote peer we are interacting with.
        remote: NodeId,
        /// If this fetch is for a particular set of `rad/sigrefs`.
        refs_at: Option<Vec<RefsAt>>,
        /// [`Config`] options when initiating the fetch protocol.
        ///
        /// [`Config`]: radicle_fetch::Config.
        config: radicle_fetch::Config,
    },
    /// Server is responding to a fetch request by uploading the
    /// specified `refspecs` sent by the client.
    Responder {
        /// Remote peer we are interacting with.
        remote: NodeId,
        /// Reporter for upload-pack progress.
        emitter: Emitter<Event>,
    },
}

impl FetchRequest {
    pub fn remote(&self) -> NodeId {
        match self {
            Self::Initiator { remote, .. } | Self::Responder { remote, .. } => *remote,
        }
    }
}

/// Fetch result of an upload or fetch.
#[derive(Debug)]
pub enum FetchResult {
    Initiator {
        /// Repo fetched.
        rid: RepoId,
        /// Fetch result, including remotes fetched.
        result: Result<fetch::FetchResult, FetchError>,
    },
    Responder {
        /// Repo requested.
        rid: Option<RepoId>,
        /// Upload result.
        result: Result<(), UploadError>,
    },
}
