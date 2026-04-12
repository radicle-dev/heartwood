//! Errors returned by [`Reader`] methods.
//!
//! [`Reader`]: super::super::Reader

use std::path::PathBuf;

use radicle_oid::Oid;
use thiserror::Error;

use crate::git::repository::types;

/// Error returned by [`Reader::blob`].
///
/// [`Reader::blob`]: super::super::Reader::blob
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Blob {
    /// The blob was not found.
    #[error("failed to find blob '{oid}'")]
    NotFound { oid: Oid },
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl Blob {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}

/// Error returned by [`Reader::blob_at`].
///
/// [`Reader::blob_at`]: super::super::Reader::blob_at
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BlobAt {
    /// Failed to find the commit.
    #[error("failed to find commit '{commit}' to retrieve blob at {path:?}")]
    CommitNotFound { commit: Oid, path: PathBuf },
    /// Failed to get the associated tree of the commit.
    #[error("failed to get associated tree of the commit '{commit}'")]
    Tree {
        commit: Oid,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    /// Failed to get the entry at `path` in the commit's tree.
    #[error("failed to get tree entry {path:?} in the commit '{commit}'")]
    TreeEntry {
        commit: Oid,
        path: PathBuf,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    /// Failed to resolve the object at the given path.
    #[error("failed to resolve the object at {path:?} in the commit '{commit}'")]
    Object {
        commit: Oid,
        path: PathBuf,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    /// The object exists but is not a blob.
    #[error("object {oid} has type `{actual}`, expected `{expected}`")]
    TypeMismatch {
        oid: Oid,
        expected: types::ObjectKind,
        actual: String,
    },
    /// The path does not exist in the commit's tree.
    #[error("the blob identified by {path:?} does not exist in the commit '{commit}'")]
    MissingBlob { commit: Oid, path: PathBuf },
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl BlobAt {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}

/// Error returned by [`Reader::commit`].
///
/// [`Reader::commit`]: super::super::Reader::commit
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Commit {
    /// The commit was not found.
    #[error("failed to find commit '{oid}'")]
    NotFound { oid: Oid },
    /// Failed to parse the raw commit bytes.
    #[error("failed to parse commit '{oid}': {source}")]
    Parse {
        oid: Oid,
        source: radicle_git_metadata::commit::ParseError,
    },
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl Commit {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}

/// Error returned by [`Reader::exists`].
///
/// [`Reader::exists`]: super::super::Reader::exists
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Exists {
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl Exists {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}
