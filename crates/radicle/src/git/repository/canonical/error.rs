use crate::git::Oid;
use crate::git::canonical::error::{FindObjectsError, QuorumError};
use crate::git::repository::{object, reference};

/// Error returned by [`Service::propose`] and [`Service::reevaluate`].
///
/// [`Service::propose`]: super::Service::propose
/// [`Service::reevaluate`]: super::Service::reevaluate
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Update {
    #[error(transparent)]
    Quorum(#[from] QuorumError),
    #[error(transparent)]
    FindObjects(#[from] FindObjectsError),
    #[error(transparent)]
    Write(#[from] reference::error::write::WriteRef),
    #[error(transparent)]
    Read(#[from] reference::error::read::RefTarget),
    #[error(transparent)]
    ObjectKind(#[from] object::error::read::ObjectKind),
    #[error("object {0} not found")]
    ObjectNotFound(Oid),
    #[error("invalid object kind for {0}")]
    InvalidObjectKind(Oid),
}
