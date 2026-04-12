//! Errors returned by [`Reader`] methods.
//!
//! [`Reader`]: super::super::Reader

use radicle_git_ref_format::RefString;
use thiserror::Error;

/// Error returned by [`Reader::ref_target`].
///
/// [`Reader::ref_target`]: super::super::Reader::ref_target
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RefTarget {
    /// The requested reference was not found.
    #[error("failed to find reference '{0}'")]
    NotFound(RefString),
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl RefTarget {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}

/// Error returned by [`Reader::list_refs`].
///
/// [`Reader::list_refs`]: super::super::Reader::list_refs
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ListRefs {
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl ListRefs {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}

/// Error yielded by the [`Reader::list_refs`] iterator.
///
/// [`Reader::list_refs`]: super::super::Reader::list_refs
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ListReference {
    /// The reference database provided a malformed reference name.
    #[error("failed to parse reference '{name}': {source}")]
    Parse {
        name: String,
        source: radicle_git_ref_format::Error,
    },
    /// The reference could not be peeled to a target commit.
    #[error("failed to peel '{name}' to target commit: {source}")]
    Peel {
        name: radicle_git_ref_format::Qualified<'static>,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    /// An error from the underlying git library.
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl ListReference {
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::Backend(Box::new(err))
    }
}
