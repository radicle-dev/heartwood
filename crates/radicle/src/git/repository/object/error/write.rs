//! Errors returned by [`Writer`] methods.
//!
//! [`Writer`]: super::super::Writer

use std::path::PathBuf;

use radicle_oid::Oid;
use thiserror::Error;

/// Error returned by [`Writer::write_blob`].
///
/// [`Writer::write_blob`]: super::super::Writer::write_blob
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Blob {
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl Blob {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}

/// Error returned by [`Writer::write_tree`].
///
/// [`Writer::write_tree`]: super::super::Writer::write_tree
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Tree {
    /// A `BlobRef` entry references an OID that does not exist.
    #[error("blob reference '{oid}' does not exist in the object database")]
    MissingBlob { oid: Oid },
    /// Failed to write blob contents for a [`TreeEntry::Blob`] entry.
    ///
    /// [`TreeEntry::Blob`]: crate::git::repository::types::TreeEntry::Blob
    #[error("failed to write blob contents to {path:?}")]
    WriteBlob {
        path: PathBuf,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl Tree {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}

/// Error returned by [`Writer::write_commit`].
///
/// [`Writer::write_commit`]: super::super::Writer::write_commit
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Commit {
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl Commit {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}
