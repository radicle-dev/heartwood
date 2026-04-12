//! Git reference operations.
//!
//! The module provides the following traits:
//! - [`Reader`] for reading references,
//! - [`Writer`] for writing references, and
//! - [`symbolic::Writer`], which extends [`Writer`], for writing symbolic references.

pub mod error;
pub mod symbolic;

use radicle_oid::Oid;

use radicle_git_ref_format::refspec::PatternStr;
use radicle_git_ref_format::{Qualified, RefStr};

/// Read Git references.
///
/// # Target Resolution
///
/// Direct references point to a target [`Oid`]. For most references, this is a
/// commit object. In the case of annotated tags, this will be a tag object.
/// In both cases, the target returned will be a commit [`Oid`]; where, in the
/// case of an annotated tag, it is the commit of the tag itself.
///
/// Symbolic references point to another reference. These references are peeled
/// until they find the target [`Oid`] of a direct reference.
pub trait Reader {
    type References<'a>: Iterator<Item = Result<(Qualified<'static>, Oid), error::read::ListReference>>
        + 'a
    where
        Self: 'a;

    /// Resolve a reference to its target [`Oid`].
    ///
    /// Returns `None` if the reference does not exist.
    ///
    /// # Errors
    ///
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`Backend`]: error::read::RefTarget::Backend
    fn ref_target<R>(&self, name: &R) -> Result<Option<Oid>, error::read::RefTarget>
    where
        R: AsRef<RefStr>;

    /// Resolve a reference to its target [`Oid`], returning an error if it does
    /// not exist.
    ///
    /// # Errors
    ///
    /// - [`NotFound`]: The reference identified by `name` was not found.
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`NotFound`]: error::read::RefTarget::NotFound
    /// [`Backend`]: error::read::RefTarget::Backend
    fn try_ref_target<R>(&self, name: &R) -> Result<Oid, error::read::RefTarget>
    where
        R: AsRef<RefStr>,
    {
        self.ref_target(name)?
            .ok_or_else(|| error::read::RefTarget::NotFound(name.as_ref().to_ref_string()))
    }

    /// List all references matching a glob pattern.
    ///
    /// Each reference is parsed and peeled to its target commit. If either of
    /// these operations fails, it is returned in the iterator. The caller may
    /// choose to log these failures and skip the entry.
    ///
    /// # Errors
    ///
    /// - [`Backend`]: An unexpected error when initialising the reference
    ///   iterator.
    ///
    /// The iterator itself yields [`ListReference`] for per-reference
    /// failures:
    /// - [`Parse`]: A reference name could not be parsed as a [`Qualified`].
    /// - [`Peel`]: A reference could not be peeled to a target commit.
    /// - [`ListReference::Backend`]: An unexpected error during iteration.
    ///
    /// [`Backend`]: error::read::ListRefs::Backend
    /// [`ListReference`]: error::read::ListReference
    /// [`Parse`]: error::read::ListReference::Parse
    /// [`Peel`]: error::read::ListReference::Peel
    /// [`ListReference::Backend`]: error::read::ListReference::Backend
    fn list_refs<'a, P>(
        &'a self,
        pattern: &P,
    ) -> Result<Self::References<'a>, error::read::ListRefs>
    where
        P: AsRef<PatternStr>;
}

/// The mode of operation for writing a reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// Set the reference to the given `target`, only if the reference does not
    /// already exist.
    Create { target: Oid },
    /// Set the reference to the given `target`, the reference may exist
    /// already.
    Upsert { target: Oid },
    /// Set the reference to the given `target`, only if the reference's
    /// current value matches `expected`.
    Cas { target: Oid, expected: Oid },
}

impl Target {
    /// Construct the [`Create`] variant, which creates a new reference pointing
    /// to the `target`. This variant will only succeed if the reference
    /// pointing to `target` does not already exist.
    ///
    /// [`Create`]: Target::Create
    pub fn create(target: Oid) -> Self {
        Self::Create { target }
    }

    /// Construct the [`Upsert`] variant, which creates a new reference pointing
    /// to the `target`. This variant will succeed even if the reference
    /// pointing to `target` already exists.
    ///
    /// [`Upsert`]: Target::Upsert
    pub fn upsert<R>(target: Oid) -> Self
    where
        R: AsRef<RefStr>,
    {
        Self::Upsert { target }
    }

    /// Construct the [`Cas`] variant, which creates a new reference pointing to
    /// the `target`. This variant will succeed only when the `expected` value
    /// matches the previously existing target value.
    ///
    /// [`Cas`]: Target::Cas
    pub fn cas(target: Oid, expected: Oid) -> Self {
        Self::Cas { target, expected }
    }

    /// The target [`Oid`] that the reference should point to after the write.
    pub fn target(&self) -> Oid {
        match self {
            Self::Create { target } | Self::Upsert { target } | Self::Cas { target, .. } => *target,
        }
    }
}

/// Write Git references.
pub trait Writer {
    /// Set a reference to the given [`Target`].
    ///
    /// # Errors
    ///
    /// - [`MissingTarget`]: The target [`Oid`] does not exist in the object
    ///   database.
    /// - [`ReferenceExists`]: The reference already exists (for
    ///   [`Target::Create`]).
    /// - [`CasFailed`]: The reference's current value did not match the
    ///   expected value (for [`Target::Cas`]).
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`MissingTarget`]: error::write::WriteRef::MissingTarget
    /// [`ReferenceExists`]: error::write::WriteRef::ReferenceExists
    /// [`CasFailed`]: error::write::WriteRef::CasFailed
    /// [`Backend`]: error::write::WriteRef::Backend
    fn write_ref<R>(
        &self,
        name: &R,
        target: Target,
        reflog: &str,
    ) -> Result<(), error::write::WriteRef>
    where
        R: AsRef<RefStr>;

    /// Delete a reference from the Git repository.
    ///
    /// This operation must be idempotent, i.e. successive deletes of the same
    /// reference name must succeed.
    ///
    /// # Errors
    ///
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`Backend`]: error::write::DeleteRef::Backend
    fn delete_ref<R>(&self, name: &R) -> Result<(), error::write::DeleteRef>
    where
        R: AsRef<RefStr>;
}
