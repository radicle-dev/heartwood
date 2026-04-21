//! User-scoped Git reference access.
//!
//! [`Namespace`] provides read and write access to a single user's references
//! within a Git repository. Consumers work with [`Qualified`] names (e.g.
//! `refs/heads/main`); the namespace mapping (`refs/namespaces/<key>/…`) is
//! handled internally.
//!
//! [`Qualified`]: radicle_git_ref_format::Qualified

pub mod error;

use std::collections::BTreeMap;

use radicle_git_ref_format::{self as fmt, Component, Qualified, refname, refspec};
use radicle_oid::Oid;

use crate::prelude::Did;

use super::reference;

/// The set of references that exist for a user.
///
/// See [`Namespace::references`].
pub struct References {
    inner: BTreeMap<Qualified<'static>, Oid>,
}

impl References {
    fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }

    fn insert(&mut self, refname: Qualified<'static>, oid: Oid) {
        self.inner.insert(refname, oid);
    }
}

impl References {
    /// Get the target [`Oid`] of the given `refname`, if it exists.
    pub fn target_of(&self, refname: &Qualified<'static>) -> Option<&Oid> {
        self.inner.get(refname)
    }
}

impl<'a> IntoIterator for &'a References {
    type Item = (&'a Qualified<'static>, &'a Oid);
    type IntoIter = std::collections::btree_map::Iter<'a, Qualified<'static>, Oid>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl IntoIterator for References {
    type Item = (Qualified<'static>, Oid);
    type IntoIter = std::collections::btree_map::IntoIter<Qualified<'static>, Oid>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

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

    /// List all references for this user matching a glob pattern.
    ///
    /// The `pattern` is relative to the user's namespace. For example,
    /// `refs/*` matches all references, and `refs/heads/*` matches only
    /// branches.
    ///
    /// Each returned [`Qualified`] has the namespace stripped — callers see
    /// `refs/heads/main`, not `refs/namespaces/<key>/refs/heads/main`.
    ///
    /// Per-reference failures (parse or peel errors) are logged and skipped.
    ///
    /// # Errors
    ///
    /// - [`ListRefs`]: An unexpected error when initialising the iterator.
    ///
    /// [`ListRefs`]: error::References::ListRefs
    pub fn references(
        &self,
        pattern: &refspec::PatternStr,
    ) -> Result<References, error::References> {
        let namespaced = refname!("refs/namespaces")
            .join(Component::from(self.did.as_key()))
            .to_pattern(pattern);

        let refs = self.repo.list_refs(&namespaced)?;
        let references = refs.fold(References::new(), |mut refs, entry| {
            match entry {
                Ok((name, oid)) => {
                    if let Some(ns) = name.to_namespaced() {
                        refs.insert(ns.strip_namespace(), oid);
                    }
                }
                Err(e) => {
                    log::warn!("Skipping reference: {e}");
                }
            }
            refs
        });
        Ok(references)
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
