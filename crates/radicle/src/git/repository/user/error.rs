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
