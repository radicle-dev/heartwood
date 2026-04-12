//! Errors returned by [`Ancestry`] methods.
//!
//! [`Ancestry`]: super::Ancestry

use radicle_oid::Oid;
use thiserror::Error;

/// Error returned by [`Ancestry::merge_base`].
///
/// [`Ancestry::merge_base`]: super::Ancestry::merge_base
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MergeBase {
    /// One of the commits could not be found
    #[error("failed to find commit '{oid}' during merge base calculation")]
    CommitNotFound { oid: Oid },
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl MergeBase {
    pub fn backend<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Backend(Box::new(err))
    }
}

/// Error returned by [`Ancestry::is_ancestor`].
///
/// [`Ancestry::is_ancestor`]: super::Ancestry::is_ancestor
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum IsAncestor {
    /// One of the commits could not be found.
    #[error("failed to find commit '{oid}'")]
    CommitNotFound { oid: Oid },
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl IsAncestor {
    pub fn backend<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Backend(Box::new(err))
    }
}

/// Error returned by [`Ancestry::ahead_behind`].
///
/// [`Ancestry::ahead_behind`]: super::Ancestry::ahead_behind
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AheadBehind {
    /// One of the commits was not found.
    #[error("commit '{oid}' was not found")]
    CommitNotFound { oid: Oid },
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl AheadBehind {
    pub fn backend<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Backend(Box::new(err))
    }
}
