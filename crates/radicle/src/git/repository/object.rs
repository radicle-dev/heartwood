//! Git object database abstraction.
//!
//! The module provides two traits:
//! - [`Reader`] for reading objects, and
//! - [`Writer`] for writing objects

pub mod error;

use std::path::Path;

use radicle_oid::Oid;

use super::types::{Blob, Commit, TreeEntry};

/// A handle for reading Git objects from the Git object database.
pub trait Reader {
    /// Find a blob by its [`Oid`].
    ///
    /// Returns `None` if the blob does not exist.
    ///
    /// # Errors
    ///
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`Backend`]: error::read::Blob::Backend
    fn blob(&self, oid: Oid) -> Result<Option<Blob>, error::read::Blob>;

    /// Find a blob by its [`Oid`], returning an error if it does not exist.
    ///
    /// # Errors
    ///
    /// - [`NotFound`]: The blob identified by `oid` does not exist.
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`NotFound`]: error::read::Blob::NotFound
    /// [`Backend`]: error::read::Blob::Backend
    fn try_blob(&self, oid: Oid) -> Result<Blob, error::read::Blob> {
        self.blob(oid)?.ok_or(error::read::Blob::NotFound { oid })
    }

    /// Find a blob at a `path` within a commit's tree.
    ///
    /// Returns `None` if the path does not exist in the commit's tree.
    ///
    /// # Errors
    ///
    /// - [`CommitNotFound`]: The commit identified by `commit` does not exist.
    /// - [`Tree`]: Failed to get the commit's tree.
    /// - [`TreeEntry`]: Failed to look up the entry at `path` in the tree.
    /// - [`Object`]: The entry was found but failed to resolve to an object.
    /// - [`TypeMismatch`]: The resolved object is not a blob.
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`CommitNotFound`]: error::read::BlobAt::CommitNotFound
    /// [`Tree`]: error::read::BlobAt::Tree
    /// [`TreeEntry`]: error::read::BlobAt::TreeEntry
    /// [`Object`]: error::read::BlobAt::Object
    /// [`TypeMismatch`]: error::read::BlobAt::TypeMismatch
    /// [`Backend`]: error::read::BlobAt::Backend
    fn blob_at<P>(&self, commit: Oid, path: &P) -> Result<Option<Blob>, error::read::BlobAt>
    where
        P: AsRef<Path>;

    /// Find a blob at a `path` within a commit's tree, returning an error if
    /// the path does not exist.
    ///
    /// # Errors
    ///
    /// - [`CommitNotFound`]: The commit identified by `commit` does not exist.
    /// - [`MissingBlob`]: The path does not exist in the commit's tree.
    /// - [`Tree`]: Failed to get the commit's tree.
    /// - [`TreeEntry`]: Failed to look up the entry at `path` in the tree.
    /// - [`Object`]: The entry was found but failed to resolve to an object.
    /// - [`TypeMismatch`]: The resolved object is not a blob.
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`CommitNotFound`]: error::read::BlobAt::CommitNotFound
    /// [`MissingBlob`]: error::read::BlobAt::MissingBlob
    /// [`Tree`]: error::read::BlobAt::Tree
    /// [`TreeEntry`]: error::read::BlobAt::TreeEntry
    /// [`Object`]: error::read::BlobAt::Object
    /// [`TypeMismatch`]: error::read::BlobAt::TypeMismatch
    /// [`Backend`]: error::read::BlobAt::Backend
    fn try_blob_at<P>(&self, commit: Oid, path: &P) -> Result<Blob, error::read::BlobAt>
    where
        P: AsRef<Path>,
    {
        self.blob_at(commit, path)?
            .ok_or_else(|| error::read::BlobAt::MissingBlob {
                commit,
                path: path.as_ref().to_path_buf(),
            })
    }

    /// Read a commit by its [`Oid`].
    ///
    /// Returns `None` if the commit does not exist.
    ///
    /// # Errors
    ///
    /// - [`Parse`]: The object was found but could not be parsed as a commit.
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`Parse`]: error::read::Commit::Parse
    /// [`Backend`]: error::read::Commit::Backend
    fn commit(&self, oid: Oid) -> Result<Option<Commit>, error::read::Commit>;

    /// Read a commit by its [`Oid`], returning an error if it does not exist.
    ///
    /// # Errors
    ///
    /// - [`NotFound`]: The commit identified by `oid` does not exist.
    /// - [`Parse`]: The object was found but could not be parsed as a commit.
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`NotFound`]: error::read::Commit::NotFound
    /// [`Parse`]: error::read::Commit::Parse
    /// [`Backend`]: error::read::Commit::Backend
    fn try_commit(&self, oid: Oid) -> Result<Commit, error::read::Commit> {
        self.commit(oid)?
            .ok_or(error::read::Commit::NotFound { oid })
    }

    /// Check whether an object exists in the object database.
    ///
    /// # Errors
    ///
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`Backend`]: error::read::Exists::Backend
    fn exists(&self, oid: Oid) -> Result<bool, error::read::Exists>;
}

/// Write Git objects to the Git object database.
///
/// Every method returns the content-addressed [`Oid`] of the written object.
pub trait Writer {
    /// Write a blob given its raw bytes content.
    ///
    /// # Errors
    ///
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`Backend`]: error::write::Blob::Backend
    fn write_blob(&self, content: &[u8]) -> Result<Oid, error::write::Blob>;

    /// Write a tree from a set of entries.
    ///
    /// [`TreeEntry::Blob`] entries have their content written as blobs first.
    /// [`TreeEntry::BlobRef`] entries reference existing blobs by [`Oid`].
    ///
    /// # Errors
    ///
    /// - [`MissingBlob`]: A [`TreeEntry::BlobRef`] references an [`Oid`] that
    ///   does not exist in the object database.
    /// - [`WriteBlob`]: Failed to write a blob for a [`TreeEntry::Blob`] entry.
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`MissingBlob`]: error::write::Tree::MissingBlob
    /// [`WriteBlob`]: error::write::Tree::WriteBlob
    /// [`Backend`]: error::write::Tree::Backend
    fn write_tree(&self, entries: &[TreeEntry]) -> Result<Oid, error::write::Tree>;

    /// Write a commit from raw bytes.
    ///
    /// The caller is responsible for producing valid Git commit bytes
    /// (e.g. via [`radicle_git_metadata`]).  This is necessary for signed
    /// commits where the exact byte representation must be controlled.
    ///
    /// # Errors
    ///
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`Backend`]: error::write::Commit::Backend
    fn write_commit(&self, bytes: &[u8]) -> Result<Oid, error::write::Commit>;
}
