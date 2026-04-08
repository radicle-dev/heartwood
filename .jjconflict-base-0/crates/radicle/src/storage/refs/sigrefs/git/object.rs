//! Traits for interacting with Git objects, necessary for implementing Radicle
//! Signed References.
// TODO(finto): I think these are more generally useful than just being used for
// Signed References. They might be worth moving into a crate,
// `radicle-git-traits`, but for now they can live here.

pub mod error;

use std::path::{Path, PathBuf};

use radicle_oid::Oid;

/// A Git blob object, returned by [`Reader::read_blob`].
pub struct Blob {
    /// The [`Oid`] of the Git blob.
    pub oid: Oid,
    /// The contents of the Git blob.
    pub bytes: Vec<u8>,
}

/// Git object reader, generally a Git repository, or its corresponding Object
/// Database (ODB).
pub trait Reader {
    /// Read the raw bytes of a commit object identified by `oid`.
    ///
    /// Returns `None` if no such object exists.
    ///
    /// # Errors
    ///
    /// - [`error::ReadCommit::IncorrectObject`]: the object identified by the
    ///   [`Oid`] was found, but was not a commit.
    /// - [`error::ReadCommit::Other`]: failed to read the Git commit.
    fn read_commit(&self, oid: &Oid) -> Result<Option<Vec<u8>>, error::ReadCommit>;

    /// Read the raw bytes of the blob at `path` within the tree of `commit`.
    ///
    /// Returns `None` if the path does not exist in that tree.
    ///
    /// # Errors
    ///
    /// - [`error::ReadBlob::CommitNotFound`]: failed to find the commit
    ///   identified by the [`Oid`].
    /// - [`error::ReadBlob::IncorrectObject`]: the object identified by the
    ///   [`Oid`] was found, but was not a commit.
    /// - [`error::ReadBlob::Other`]: failed to read the Git blob.
    fn read_blob(&self, commit: &Oid, path: &Path) -> Result<Option<Blob>, error::ReadBlob>;
}

/// Input to the [`Writer::write_tree`] method.
///
/// The entry describes where in the Git tree to write the [`Refs`] content
/// blob.
///
/// [`Refs`]: crate::storage::refs::Refs
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RefsEntry {
    /// Path in the Git tree to write to.
    pub path: PathBuf,
    /// The contents of the Git blob.
    pub content: Vec<u8>,
}

/// Input to the [`Writer::write_tree`] method.
///
/// The entry describes where in the Git tree to write the signature content
/// blob.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SignatureEntry {
    /// Path in the Git tree to write to.
    pub path: PathBuf,
    /// The contents of the Git blob.
    pub content: Vec<u8>,
}

/// Git object writer, generally a Git repository, or its corresponding Object
/// Database (ODB).
pub trait Writer {
    /// Write the [`RefsEntry`] and [`SignatureEntry`] to two separate Git blobs
    /// within a shared Git tree.
    ///
    /// Returns the [`Oid`] of the Git tree.
    ///
    /// # Errors
    ///
    /// - [`error::WriteTree::Refs`]: failed to write the references Git blob.
    /// - [`error::WriteTree::Signature`]: failed to write the signature Git blob.
    /// - [`error::WriteTree::Write`]: failed to write the Git tree.
    fn write_tree(
        &self,
        refs: RefsEntry,
        signature: SignatureEntry,
    ) -> Result<Oid, error::WriteTree>;

    /// Write the given Git commit, as bytes, to the Git object database.
    ///
    /// Returns the [`Oid`] of the Git commit.
    ///
    /// # Errors
    ///
    /// - [`error::WriteCommit`]: failed to write the Git commit.
    fn write_commit(&self, bytes: &[u8]) -> Result<Oid, error::WriteCommit>;
}
