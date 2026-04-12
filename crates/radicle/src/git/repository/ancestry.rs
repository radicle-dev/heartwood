//! Git commit graph ancestry trait.
//!
//! [`Ancestry`] provides merge-base, ancestor checks, and ahead/behind counts.

pub mod error;

use radicle_oid::Oid;

/// The result of [`Ancestry::ahead_behind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AheadBehind {
    /// The given commit was ahead of the upstream by this many commits.
    pub ahead: usize,
    /// The given commit was behind the upstream by this many commits.
    pub behind: usize,
}

/// Git commit graph operations.
///
/// Provides merge-base computation and ancestor checks.
pub trait Ancestry {
    /// Find the merge base (common ancestor) of two commits.
    ///
    /// Returns `Ok(None)` if there is no common ancestor.
    ///
    /// # Errors
    ///
    /// - [`CommitNotFound`]: One of the commits was not found.
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`CommitNotFound`]: error::MergeBase::CommitNotFound
    /// [`Backend`]: error::MergeBase::Backend
    fn merge_base(&self, a: Oid, b: Oid) -> Result<Option<Oid>, error::MergeBase>;

    /// Check whether `ancestor` is an ancestor of `head`.
    ///
    /// # Errors
    ///
    /// - [`CommitNotFound`]: One of the commits was not found.
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`CommitNotFound`]: error::IsAncestor::CommitNotFound
    /// [`Backend`]: error::IsAncestor::Backend
    fn is_ancestor(&self, ancestor: Oid, head: Oid) -> Result<bool, error::IsAncestor>;

    /// Count how many commits `commit` is ahead of and behind `upstream`.
    ///
    /// # Errors
    ///
    /// - [`CommitNotFound`]: One of the commits was not found.
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`CommitNotFound`]: error::AheadBehind::CommitNotFound
    /// [`Backend`]: error::AheadBehind::Backend
    fn ahead_behind(&self, commit: Oid, upstream: Oid) -> Result<AheadBehind, error::AheadBehind>;
}
