//! Domain types for Git repository operations.
//!
//! # Objects
//!
//! The following object types are defined in this module:
//! - [`Blob`]
//! - [`Commit`]
//! - [`TreeEntry`]

use core::fmt;
use std::path::PathBuf;

use radicle_git_metadata as metadata;
use radicle_oid::Oid;

/// An enumeration of the kinds of Git objects
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ObjectKind {
    /// A Git blob object.
    Blob,
    /// A Git tree object.
    Tree,
    /// A Git commit object.
    Commit,
    /// A Git tag object.
    Tag,
}

impl fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectKind::Blob => f.write_str("blob"),
            ObjectKind::Tree => f.write_str("tree"),
            ObjectKind::Commit => f.write_str("commit"),
            ObjectKind::Tag => f.write_str("tag"),
        }
    }
}

/// A resolved Git object.
pub struct Object {
    /// The content-addressed identifier of the object.
    pub oid: Oid,
    /// The kind of the object.
    pub kind: ObjectKind,
}

/// A Git blob object.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Blob {
    /// The content-addressed identifier of the blob.
    pub oid: Oid,
    /// The blob's content.
    pub content: Vec<u8>,
}

/// A Git commit, with all metadata parsed.
///
/// This is a type alias for [`radicle_git_metadata::commit::CommitData`]
/// specialised to [`Oid`] for both tree and parent identifiers.
pub type Commit = metadata::commit::CommitData<Oid, Oid>;

/// An entry to be written into a Git tree.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TreeEntry {
    /// A blob entry with inline content.
    ///
    /// The contents of the blob must be written to the Git repository before
    /// creating the tree entry.
    Blob {
        /// The path of the entry within the tree.
        ///
        /// Multi-component paths (e.g. `a/b/c.txt`) are supported;
        /// intermediate sub-trees are created automatically.
        path: PathBuf,
        /// The contents of the blob.
        content: Vec<u8>,
    },
    /// A reference to an existing blob by [`Oid`].
    ///
    /// Used when the blob already exists in the Git object database.
    BlobRef {
        /// The path of the entry within the tree.
        ///
        /// Multi-component paths (e.g. `a/b/c.txt`) are supported;
        /// intermediate sub-trees are created automatically.
        path: PathBuf,
        /// The [`Oid`] of the existing blob.
        oid: Oid,
    },
}
