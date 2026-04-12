//! Errors returned by [`Writer`] and [`symbolic::Writer`] methods.
//!
//! [`Writer`]: super::super::Writer
//! [`symbolic::Writer`]: super::super::symbolic::Writer

use radicle_git_ref_format::RefString;
use radicle_oid::Oid;
use thiserror::Error;

/// Error returned by [`Writer::write_ref`].
///
/// [`Writer::write_ref`]: super::super::Writer::write_ref
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WriteRef {
    /// Compare-and-swap failed.
    #[error(
        "failed to update reference '{name}' due to compare-and-swap failure with expected value {expected}"
    )]
    CasFailed { name: String, expected: Oid },
    /// The target OID does not exist in the object database.
    #[error("target object {target} not found when writing reference `{name}`")]
    MissingTarget { name: String, target: Oid },
    /// The reference already exists (for create-only writes).
    #[error("reference '{name}' already exists")]
    ReferenceExists { name: String },
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl WriteRef {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}

/// Error returned by [`Writer::delete_ref`].
///
/// [`Writer::delete_ref`]: super::super::Writer::delete_ref
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DeleteRef {
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl DeleteRef {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}

/// Error returned by [`Writer::write_symbolic_ref`].
///
/// [`Writer::write_symbolic_ref`]: super::super::symbolic::Writer::write_symbolic_ref
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WriteSymbolicRef {
    /// The target reference does not exist.
    #[error("could not create symbolic reference '{name}' due to missing target '{target}'")]
    MissingTarget { name: RefString, target: RefString },
    /// The named reference already exists.
    #[error(
        "could not create symbolic reference from '{name}' to '{target}', the reference already exists"
    )]
    ReferenceExists { name: RefString, target: RefString },
    /// Compare-and-swap failed.
    #[error(
        "failed to update reference '{name}' due to compare-and-swap failure with expected value {expected}"
    )]
    CasFailed {
        name: RefString,
        expected: RefString,
    },
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl WriteSymbolicRef {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}
