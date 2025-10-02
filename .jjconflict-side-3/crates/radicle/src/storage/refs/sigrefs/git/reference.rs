//! Traits for interacting with Git references, necessary for implementing
//! Radicle Signed References.
// TODO(finto): I think these are more generally useful than just being used for
// Signed References. They might be worth moving into a crate,
// `radicle-git-traits`, but for now they can live here.

pub mod error;

use radicle_oid::Oid;

use crate::git;

/// Git reference reader, generally a Git repository, or its corresponding Reference
/// Database (Ref DB).
pub trait Reader {
    /// Find the head [`Oid`] of the sigrefs reference for the given namespace.
    ///
    /// Returns `None` if the reference does not yet exist.
    /// # Errors
    ///
    /// - [`error::FindReference`]: failed to write the Git reference.
    fn find_reference(
        &self,
        reference: &git::fmt::Namespaced,
    ) -> Result<Option<Oid>, error::FindReference>;
}

/// Git reference writer, generally a Git repository, or its corresponding Reference
/// Database (Ref DB).
pub trait Writer {
    /// Write the given commit [`Oid`], and its parent, to the given
    /// `reference`.
    ///
    /// The `reflog` given can used as the Git reflog message of the reference.
    ///
    /// # Concurrency
    ///
    /// It is up to the implementer to ensure the safety of writing the
    /// reference safely in a concurrent environment.
    ///
    /// # Errors
    ///
    /// - [`error::WriteReference`]: failed to write the Git reference.
    fn write_reference(
        &self,
        reference: &git::fmt::Namespaced,
        commit: Oid,
        parent: Option<Oid>,
        reflog: String,
    ) -> Result<(), error::WriteReference>;
}
