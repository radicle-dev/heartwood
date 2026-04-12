//! Errors returned by [`Revwalk`] methods and iterators.
//!
//! [`Revwalk`]: super::Revwalk

use radicle_oid::Oid;
use thiserror::Error;

/// Error returned by [`Revwalk::revwalk_oids`] and
/// [`Revwalk::revwalk_commits`] when initialising the walk.
///
/// [`Revwalk::revwalk_oids`]: super::Revwalk::revwalk_oids
/// [`Revwalk::revwalk_commits`]: super::Revwalk::revwalk_commits
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Init {
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl Init {
    pub fn backend<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Backend(Box::new(err))
    }
}

/// Error yielded by the [`Revwalk::RevwalkOids`] iterator.
///
/// [`Revwalk::RevwalkOids`]: super::Revwalk::RevwalkOids
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Oids {
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl Oids {
    pub fn backend<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Backend(Box::new(err))
    }
}

/// Error yielded by the [`Revwalk::RevwalkCommits`] iterator.
///
/// [`Revwalk::RevwalkCommits`]: super::Revwalk::RevwalkCommits
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Commit {
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
    pub fn backend<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Backend(Box::new(err))
    }
}
