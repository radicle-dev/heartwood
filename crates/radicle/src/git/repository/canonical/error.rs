use crate::git::Oid;
use crate::git::repository::{object, reference};

pub use crate::git::repository::canonical::eval::error::{
    ConvergesError, FindObjectsError, MergeBaseError, QuorumError,
};

/// Errors that occur during read-only canonical evaluation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Quorum(#[from] QuorumError),
    #[error(transparent)]
    FindObjects(#[from] FindObjectsError),
}

/// Error returned by [`Namespace::propose`] and [`Namespace::reevaluate`].
///
/// [`Namespace::propose`]: super::Namespace::propose
/// [`Namespace::reevaluate`]: super::Namespace::reevaluate
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Update {
    #[error(transparent)]
    Evaluate(#[from] Error),
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

// Convenience implementations to keep `propose` clean
impl From<QuorumError> for Update {
    fn from(e: QuorumError) -> Self {
        Self::Evaluate(e.into())
    }
}

impl From<FindObjectsError> for Update {
    fn from(e: FindObjectsError) -> Self {
        Self::Evaluate(e.into())
    }
}
