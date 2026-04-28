use crate::git::repository::reference;

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
