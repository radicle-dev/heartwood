//! User-scoped Git reference access.
//!
//! [`Namespace`] provides read and write access to a single user's references
//! within a Git repository. Consumers work with [`Qualified`] names (e.g.
//! `refs/heads/main`); the namespace mapping (`refs/namespaces/<key>/…`) is
//! handled internally.
//!
//! [`Qualified`]: radicle_git_ref_format::Qualified

pub mod error;

#[cfg(test)]
mod test;

use std::collections::BTreeMap;

use crypto::PublicKey;
use radicle_git_ref_format as fmt;
use radicle_git_ref_format::{Component, Qualified, RefStr};
use radicle_git_ref_format::{pattern, refname, refspec};
use radicle_oid::Oid;

use crate::prelude::Did;

use super::ObjectKind;
use super::types::Object;
use super::{object, reference};

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

impl<'a, R: reference::Reader + object::Reader> Namespace<'a, R> {
    /// Find the object that is pointed to by `refname`, in the user namespace.
    ///
    /// The resulting object should either be an [`ObjectKind::Commit`] or
    /// [`ObjectKind::Tag`], but other [`ObjectKind`]'s may be returned.
    ///
    /// # Errors
    ///
    /// - [`FindObject::RefTarget`]: An error occurred when attempting to resolve the
    ///   [`Oid`] of the reference, identified by `refname`.
    /// - [`FindObject::ObjectKind`]: An error occurred when attempting to resolve the
    ///   [`ObjectKind`] of the [`Oid`] that the reference is pointing to.
    ///
    /// [`FindObject::RefTarget`]: error::FindObject::RefTarget
    /// [`FindObject::ObjectKind`]: error::FindObject::ObjectKind
    pub fn find_object(&self, refname: &Qualified) -> Result<Option<Object>, error::FindObject> {
        let oid = self
            .ref_target(refname)
            .map_err(|err| error::FindObject::RefTarget {
                refname: refname.clone().to_owned(),
                source: err,
            })?;
        oid.and_then(|oid| self.object(refname, oid).transpose())
            .transpose()
    }

    fn object(&self, refname: &Qualified, oid: Oid) -> Result<Option<Object>, error::FindObject> {
        self.object_kind(oid)
            .map_err(|err| error::FindObject::ObjectKind {
                oid,
                refname: refname.clone().to_owned(),
                source: err,
            })
            .map(|kind| kind.map(|kind| Object { oid, kind }))
    }

    fn object_kind(&self, oid: Oid) -> Result<Option<ObjectKind>, object::error::read::ObjectKind> {
        self.repo.object_kind(oid)
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

/// Discovery of users (namespaces) in a Git repository.
///
/// [`Namespaces`] provides iterator-based access to the [`Did`]s that have
/// references in the repository. The optional `filter_by` suffix narrows the
/// search — for example, passing `refs/rad/sigrefs` limits discovery to
/// users that have a signed-refs branch.
pub struct Namespaces<'a, R> {
    repo: &'a R,
}

/// Provide a filter for [`Namespaces::dids`] and
/// [`Namespaces::dids_with_errors`].
pub enum FilterBy<'a> {
    /// Provide a suffix to filter the [`Did`]s by.
    Suffix(&'a RefStr),
    /// No filter is provided, returning all [`Did`]s.
    Empty,
}

impl<'a> FilterBy<'a> {
    /// Constructs a [`FilterBy::Suffix`].
    pub fn suffix<R>(suffix: &'a R) -> Self
    where
        R: AsRef<RefStr>,
    {
        Self::Suffix(suffix.as_ref())
    }

    /// Constructs a [`FilterBy::Empty`].
    pub fn empty() -> Self {
        Self::Empty
    }
}

impl<'a, R> Namespaces<'a, R>
where
    R: reference::Reader,
{
    /// Create a new [`Namespaces`] handle backed by `repo`.
    pub fn new(repo: &'a R) -> Self {
        Self { repo }
    }

    /// Iterate over discovered [`Did`]s, logging and skipping errors.
    ///
    /// When `filter_by` is [`Empty`], all namespaces are returned. When a [`Suffix`]
    /// is provided (e.g. `refs/rad/sigrefs`), only namespaces containing a
    /// reference matching that suffix are returned.
    ///
    /// **Note**: the returned [`Did`]s may contain duplicates when
    /// `filter_by` is [`Empty`], since a single namespace can contain multiple
    /// references.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference iterator cannot be initialised.
    ///
    /// [`Empty`]: FilterBy::Empty
    /// [`Suffix`]: FilterBy::Suffix
    pub fn dids(self, filter_by: FilterBy<'_>) -> Result<Dids<R::References<'a>>, error::Dids> {
        let inner = self.refs_iter(filter_by)?;
        Ok(Dids { inner })
    }

    /// Like [`Self::dids`], but yields `Result<Did, NamespaceError>` so the
    /// caller can handle per-reference failures.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference iterator cannot be initialised.
    pub fn dids_with_errors(
        self,
        filter_by: FilterBy<'_>,
    ) -> Result<DidsWithErrors<R::References<'a>>, error::Dids> {
        let inner = self.refs_iter(filter_by)?;
        Ok(DidsWithErrors { inner })
    }

    fn refs_iter(
        &self,
        filter_by: FilterBy<'_>,
    ) -> Result<R::References<'a>, reference::error::read::ListRefs> {
        let pattern = pattern!("refs/namespaces/*");
        let pattern = match filter_by {
            FilterBy::Suffix(suffix) => pattern.join(suffix),
            FilterBy::Empty => pattern,
        };
        self.repo.list_refs(&pattern)
    }
}

/// Extract a [`Did`] from a namespaced [`Qualified`] reference name.
///
/// Returns `None` if the reference is not namespaced.
fn to_did(refname: &Qualified<'_>) -> Option<Result<Did, crypto::PublicKeyError>> {
    let namespaced = refname.to_namespaced()?;
    let did = namespaced
        .namespace()
        .as_str()
        .parse::<PublicKey>()
        .map(Did::from);
    Some(did)
}

/// Iterator yielding [`Did`]s, logging and skipping errors.
///
/// Produced by [`Namespaces::dids`].
pub struct Dids<I> {
    inner: I,
}

impl<I> Iterator for Dids<I>
where
    I: Iterator<Item = Result<(Qualified<'static>, Oid), reference::error::read::ListReference>>,
{
    type Item = Did;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.inner.next()? {
                Ok((name, _)) => match to_did(&name) {
                    Some(Ok(did)) => return Some(did),
                    Some(Err(e)) => {
                        log::warn!(target: "radicle", "Skipping namespace with invalid key: {e}");
                    }
                    None => {}
                },
                Err(e) => {
                    log::warn!(target: "radicle", "Skipping malformed reference: {e}");
                }
            }
        }
    }
}

/// Error produced by [`DidsWithErrors`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NamespaceError {
    /// The namespace component could not be parsed as a [`Did`].
    #[error("invalid namespace key: {0}")]
    Did(#[from] crypto::PublicKeyError),
    /// A reference could not be read or resolved.
    #[error(transparent)]
    Reference(#[from] reference::error::read::ListReference),
}

/// Iterator yielding `Result<Did, NamespaceError>`.
///
/// Produced by [`Namespaces::dids_with_errors`].
pub struct DidsWithErrors<I> {
    inner: I,
}

impl<I> Iterator for DidsWithErrors<I>
where
    I: Iterator<Item = Result<(Qualified<'static>, Oid), reference::error::read::ListReference>>,
{
    type Item = Result<Did, NamespaceError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.inner.next()? {
                Ok((name, _)) => match to_did(&name) {
                    Some(Ok(did)) => return Some(Ok(did)),
                    Some(Err(e)) => return Some(Err(NamespaceError::Did(e))),
                    None => continue,
                },
                Err(e) => return Some(Err(NamespaceError::Reference(e))),
            }
        }
    }
}
