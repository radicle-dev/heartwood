//! User-scoped Git reference access.
//!
//! [`Namespace`] provides read and write access to a single user's references
//! within a Git repository. Consumers work with [`Qualified`] names (e.g.
//! `refs/heads/main`); the namespace mapping (`refs/namespaces/<key>/…`) is
//! handled internally.
//!
//! [`Qualified`]: radicle_git_ref_format::Qualified

use radicle_git_ref_format::{self as fmt, Qualified};
use radicle_oid::Oid;

use crate::prelude::Did;

use super::reference;

/// User-scoped reference handle.
///
/// Wraps a repository `R` and a [`Did`], translating [`Qualified`] reference
/// names into their namespaced physical location.
pub struct Namespace<'a, R> {
    did: Did,
    repo: &'a R,
}

impl<'a, R> Namespace<'a, R> {
    /// Create a new [`Namespace`] for `did` backed by `repo`.
    pub fn new(did: Did, repo: &'a R) -> Self {
        Self { did, repo }
    }

    /// The [`Did`] this handle is scoped to.
    pub fn did(&self) -> Did {
        self.did
    }

    /// Map a [`Qualified`] reference to its namespaced form.
    fn namespaced<'b>(&self, name: &Qualified<'b>) -> fmt::Namespaced<'b> {
        name.with_namespace(fmt::Component::from(self.did.as_key()))
    }
}

impl<'a, R: reference::Reader> Namespace<'a, R> {
    /// Resolve a reference to its target [`Oid`].
    ///
    /// Returns `None` if the reference does not exist for this user.
    ///
    /// # Errors
    ///
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`Backend`]: reference::error::read::RefTarget::Backend
    pub fn ref_target(
        &self,
        name: &Qualified,
    ) -> Result<Option<Oid>, reference::error::read::RefTarget> {
        self.repo.ref_target(&self.namespaced(name))
    }

    /// Resolve a reference to its target [`Oid`], returning an error if it does
    /// not exist.
    ///
    /// # Errors
    ///
    /// - [`NotFound`]: The reference does not exist for this user.
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`NotFound`]: reference::error::read::RefTarget::NotFound
    /// [`Backend`]: reference::error::read::RefTarget::Backend
    pub fn try_ref_target(
        &self,
        name: &Qualified,
    ) -> Result<Oid, reference::error::read::RefTarget> {
        self.repo.try_ref_target(&self.namespaced(name))
    }
}

impl<'a, R: reference::Writer> Namespace<'a, R> {
    /// Set a reference for this user.
    ///
    /// # Errors
    ///
    /// See [`reference::Writer::write_ref`] for error details.
    pub fn write_ref(
        &self,
        name: &Qualified,
        target: reference::Target,
        reflog: &str,
    ) -> Result<(), reference::error::write::WriteRef> {
        self.repo.write_ref(&self.namespaced(name), target, reflog)
    }

    /// Delete a reference for this user.
    ///
    /// This operation is idempotent.
    ///
    /// # Errors
    ///
    /// See [`reference::Writer::delete_ref`] for error details.
    pub fn delete_ref(&self, name: &Qualified) -> Result<(), reference::error::write::DeleteRef> {
        self.repo.delete_ref(&self.namespaced(name))
    }
}
