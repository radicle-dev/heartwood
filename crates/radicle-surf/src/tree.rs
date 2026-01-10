//! Represents git object type 'tree', i.e. like directory entries in Unix.
//! See git [doc](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects) for more details.

use std::cmp::Ordering;
use std::path::PathBuf;

use radicle_git_ext::Oid;
#[cfg(feature = "serde")]
use serde::{
    ser::{SerializeStruct as _, Serializer},
    Serialize,
};
use url::Url;

use crate::{fs, Commit, Error, Repository};

/// Represents a tree object as in git. It is essentially the content of
/// one directory. Note that multiple directories can have the same content,
/// i.e. have the same tree object. Hence this struct does not embed its path.
#[derive(Clone, Debug)]
pub struct Tree {
    /// The object id of this tree.
    id: Oid,
    /// The first descendant entries for this tree.
    entries: Vec<Entry>,
    /// The commit object that created this tree object.
    commit: Commit,
    /// The root path this tree was constructed from.
    root: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum LastCommitError {
    #[error(transparent)]
    Repo(#[from] Error),
    #[error("could not get the last commit for this entry")]
    Missing,
}

impl Tree {
    /// Creates a new tree, ensuring the `entries` are sorted.
    pub(crate) fn new(id: Oid, mut entries: Vec<Entry>, commit: Commit, root: PathBuf) -> Self {
        entries.sort();
        Self {
            id,
            entries,
            commit,
            root,
        }
    }

    pub fn object_id(&self) -> Oid {
        self.id
    }

    /// Returns the commit for which this [`Tree`] was constructed from.
    pub fn commit(&self) -> &Commit {
        &self.commit
    }

    /// Returns the commit that last touched this [`Tree`].
    pub fn last_commit(&self, repo: &Repository) -> Result<Commit, LastCommitError> {
        repo.last_commit(&self.root, self.commit().clone())?
            .ok_or(LastCommitError::Missing)
    }

    /// Returns the entries of the tree.
    pub fn entries(&self) -> &Vec<Entry> {
        &self.entries
    }
}

#[cfg(feature = "serde")]
impl Serialize for Tree {
    /// Sample output:
    /// (for `<entry_1>` and `<entry_2>` sample output, see [`Entry`])
    /// ```
    /// {
    ///   "entries": [
    ///     { <entry_1> },
    ///     { <entry_2> },
    ///   ],
    ///   "root": "src/foo",
    ///   "commit": {
    ///     "author": {
    ///       "email": "foobar@gmail.com",
    ///       "name": "Foo Bar"
    ///     },
    ///     "committer": {
    ///       "email": "noreply@github.com",
    ///       "name": "GitHub"
    ///     },
    ///     "committerTime": 1582198877,
    ///     "description": "A sample commit.",
    ///     "sha1": "b57846bbc8ced6587bf8329fc4bce970eb7b757e",
    ///     "summary": "Add a new sample"
    ///   },
    ///   "oid": "dd52e9f8dfe1d8b374b2a118c25235349a743dd2"
    /// }
    /// ```
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        const FIELDS: usize = 4;
        let mut state = serializer.serialize_struct("Tree", FIELDS)?;
        state.serialize_field("oid", &self.id)?;
        state.serialize_field("entries", &self.entries)?;
        state.serialize_field("commit", &self.commit)?;
        state.serialize_field("root", &self.root)?;
        state.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    Tree(Oid),
    Blob(Oid),
    Submodule { id: Oid, url: Option<Url> },
}

impl PartialOrd for EntryKind {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EntryKind {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (EntryKind::Submodule { .. }, EntryKind::Submodule { .. }) => Ordering::Equal,
            (EntryKind::Submodule { .. }, EntryKind::Tree(_)) => Ordering::Equal,
            (EntryKind::Tree(_), EntryKind::Submodule { .. }) => Ordering::Equal,
            (EntryKind::Tree(_), EntryKind::Tree(_)) => Ordering::Equal,
            (EntryKind::Tree(_), EntryKind::Blob(_)) => Ordering::Less,
            (EntryKind::Blob(_), EntryKind::Tree(_)) => Ordering::Greater,
            (EntryKind::Submodule { .. }, EntryKind::Blob(_)) => Ordering::Less,
            (EntryKind::Blob(_), EntryKind::Submodule { .. }) => Ordering::Greater,
            (EntryKind::Blob(_), EntryKind::Blob(_)) => Ordering::Equal,
        }
    }
}

/// An entry that can be found in a tree.
///
/// # Ordering
///
/// The ordering of a [`Entry`] is first by its `entry` where
/// [`EntryKind::Tree`]s come before [`EntryKind::Blob`]. If both kinds
/// are equal then they are next compared by the lexicographical ordering
/// of their `name`s.
#[derive(Clone, Debug)]
pub struct Entry {
    name: String,
    entry: EntryKind,
    path: PathBuf,
    /// The commit from which this entry was constructed from.
    commit: Commit,
}

impl Entry {
    pub(crate) fn new(name: String, path: PathBuf, entry: EntryKind, commit: Commit) -> Self {
        Self {
            name,
            entry,
            path,
            commit,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The full path to this entry from the root of the Git repository
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn entry(&self) -> &EntryKind {
        &self.entry
    }

    pub fn is_tree(&self) -> bool {
        matches!(self.entry, EntryKind::Tree(_))
    }

    pub fn commit(&self) -> &Commit {
        &self.commit
    }

    pub fn object_id(&self) -> Oid {
        match self.entry {
            EntryKind::Blob(id) => id,
            EntryKind::Tree(id) => id,
            EntryKind::Submodule { id, .. } => id,
        }
    }

    /// Returns the commit that last touched this [`Entry`].
    pub fn last_commit(&self, repo: &Repository) -> Result<Commit, LastCommitError> {
        repo.last_commit(&self.path, self.commit.clone())?
            .ok_or(LastCommitError::Missing)
    }
}

// To support `sort`.
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.entry
            .cmp(&other.entry)
            .then(self.name.cmp(&other.name))
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.entry == other.entry && self.name == other.name
    }
}

impl Eq for Entry {}

impl From<fs::Entry> for EntryKind {
    fn from(entry: fs::Entry) -> Self {
        match entry {
            fs::Entry::File(f) => EntryKind::Blob(f.id()),
            fs::Entry::Directory(d) => EntryKind::Tree(d.id()),
            fs::Entry::Submodule(u) => EntryKind::Submodule {
                id: u.id(),
                url: u.url().clone(),
            },
        }
    }
}

#[cfg(feature = "serde")]
impl Serialize for Entry {
    /// Sample output:
    /// ```json
    ///  {
    ///     "kind": "blob",
    ///     "commit": {
    ///       "author": {
    ///         "email": "foobar@gmail.com",
    ///         "name": "Foo Bar"
    ///       },
    ///       "committer": {
    ///         "email": "noreply@github.com",
    ///         "name": "GitHub"
    ///       },
    ///       "committerTime": 1578309972,
    ///       "description": "This is a sample file",
    ///       "sha1": "2873745c8f6ffb45c990eb23b491d4b4b6182f95",
    ///       "summary": "Add a new sample"
    ///     },
    ///     "path": "src/foo/Sample.rs",
    ///     "name": "Sample.rs",
    ///     "oid": "6d6240123a8d8ea8a8376610168a0a4bcb96afd0"
    ///   },
    /// ```
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        const FIELDS: usize = 5;
        let mut state = serializer.serialize_struct("TreeEntry", FIELDS)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field(
            "kind",
            match self.entry {
                EntryKind::Blob(_) => "blob",
                EntryKind::Tree(_) => "tree",
                EntryKind::Submodule { .. } => "submodule",
            },
        )?;
        if let EntryKind::Submodule { url: Some(url), .. } = &self.entry {
            state.serialize_field("url", url)?;
        };
        state.serialize_field("oid", &self.object_id())?;
        state.serialize_field("commit", &self.path)?;
        state.end()
    }
}
