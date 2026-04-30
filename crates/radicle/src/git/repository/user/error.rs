use radicle_git_ref_format::Qualified;
use radicle_oid::Oid;

use crate::git::repository::{object, reference};

/// Error returned by [`Namespace::references`].
///
/// [`Namespace::references`]: super::Namespace::references
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum References {
    /// Failed to initialise the reference iterator.
    #[error(transparent)]
    ListRefs(#[from] reference::error::read::ListRefs),
}

/// Error returned by [`Namespace::find_object`]
///
/// [`Namespace::find_object`]: super::Namespace::find_object
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FindObject {
    /// Failed to resolve the target of the reference.
    #[error("failed to resolve {refname}: {source}")]
    RefTarget {
        refname: Qualified<'static>,
        source: reference::error::read::RefTarget,
    },
    /// Failed to determine the kind of the object.
    #[error("failed to determine object kind of {oid}, found at {refname}: {source}")]
    ObjectKind {
        oid: Oid,
        refname: Qualified<'static>,
        source: object::error::read::ObjectKind,
    },
}

/// Error returned by [`Namespaces::dids`] and [`Namespaces::dids_with_errors`].
///
/// [`Namespaces::dids`]: super::Namespaces::dids
/// [`Namespaces::dids_with_errors`]: super::Namespaces::dids_with_errors
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Dids {
    /// Failed to initialise the reference iterator.
    #[error(transparent)]
    ListRefs(#[from] reference::error::read::ListRefs),
}
