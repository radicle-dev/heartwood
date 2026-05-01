//! Composable test fixture for building Git repositories with known state.
//!
//! [`Repository`] wraps a bare [`super::Repository`] in a [`TempDir`] and
//! provides helpers to create commits, references, and namespaced references
//! without boilerplate.
//!
//! # Example
//!
//! ```rust,ignore
//! let mut repo = Repository::new();
//! let root = repo.commit(&[], &[("file.txt", b"hello")]);
//! let child = repo.commit(&[root], &[("file.txt", b"updated")]);
//! repo.reference("refs/heads/main", child);
//! ```

use std::collections::BTreeSet;

use radicle_oid::Oid;
use tempfile::TempDir;

use crate::prelude::Did;

/// A bare Git repository in a temporary directory, with helpers for
/// constructing test state.
///
/// Use [`Repository::raw`] to get the raw handle on the underlying
/// [`super::Repository`].
///
/// For manipulating the underlying repository with fixture data see:
/// - [`Repository::commit`]
/// - [`Repository::reference`]
/// - [`Repository::namespaced_ref`]
/// - [`Repository::blob`]
pub struct Repository {
    inner: super::Repository,
    dids: BTreeSet<Did>,
    _dir: TempDir,
}

impl Default for Repository {
    fn default() -> Self {
        Self::new()
    }
}

impl Repository {
    /// Create a new empty bare repository.
    pub fn new() -> Self {
        let dir = TempDir::new().expect("failed to create temp dir");
        let inner = super::Repository::init_bare(dir.path()).expect("failed to init bare repo");
        Self {
            inner,
            dids: BTreeSet::new(),
            _dir: dir,
        }
    }

    /// Access the underlying [`super::Repository`].
    pub fn raw(&self) -> &super::Repository {
        &self.inner
    }

    /// The set of [`Did`]s registered via [`Self::namespaced_ref`].
    pub fn known_dids(&self) -> &BTreeSet<Did> {
        &self.dids
    }

    /// Create a commit with the given tree content and parent commits.
    ///
    /// `files` is a list of `(path, content)` pairs. Each path may be
    /// multi-component (e.g. `"a/b/c.txt"`) — intermediate trees are
    /// created automatically via [`super::build::TreeUpdateBuilder`].
    ///
    /// Returns the [`Oid`] of the new commit.
    pub fn commit(&self, parents: &[Oid], files: &[(&str, &[u8])]) -> Oid {
        let sig = super::Signature::new("test", "test@test", &super::Time::new(0, 0))
            .expect("valid signature");

        let tree_oid = self.build_tree(files);
        let tree = self.inner.find_tree(tree_oid).expect("tree just written");

        let parent_commits: Vec<super::Commit<'_>> = parents
            .iter()
            .map(|oid| {
                self.inner
                    .find_commit((*oid).into())
                    .unwrap_or_else(|_| panic!("parent commit {oid} not found"))
            })
            .collect();
        let parent_refs: Vec<&super::Commit<'_>> = parent_commits.iter().collect();

        self.inner
            .commit(None, &sig, &sig, "test commit", &tree, &parent_refs)
            .expect("failed to create commit")
            .into()
    }

    /// Create a tag with the given `name`, pointing to the object identified by
    /// the given [`Oid`].
    ///
    /// Returns the [`Oid`] of the tag object.
    pub fn tag(&self, name: &str, oid: Oid, force: bool) -> Oid {
        let sig = super::Signature::new("test", "test@test", &super::Time::new(0, 0))
            .expect("valid signature");
        let target = self.inner.find_object(oid.into(), None).unwrap();
        self.inner
            .tag(name, &target, &sig, "fixture tag", force)
            .unwrap()
            .into()
    }

    /// Create a direct reference pointing to `target`.
    ///
    /// Panics if the reference already exists. Use [`Self::raw`] for
    /// more control.
    pub fn reference(&self, name: &str, target: Oid) {
        self.inner
            .reference(name, target.into(), false, "fixture")
            .unwrap_or_else(|e| panic!("failed to create reference {name}: {e}"));
    }

    /// Create a namespaced reference for a [`Did`].
    ///
    /// The `refname` is a qualified name like `refs/heads/main`. It is
    /// prefixed with `refs/namespaces/<key>/` internally.
    ///
    /// The [`Did`] is recorded in [`Self::known_dids`].
    pub fn namespaced_ref(&mut self, did: Did, refname: &str, target: Oid) {
        let key = did.as_key();
        let full = format!("refs/namespaces/{key}/{refname}");
        self.inner
            .reference(&full, target.into(), false, "fixture")
            .unwrap_or_else(|e| panic!("failed to create namespaced ref {full}: {e}"));
        self.dids.insert(did);
    }

    /// Write a blob and return its [`Oid`].
    pub fn blob(&self, content: &[u8]) -> Oid {
        self.inner
            .blob(content)
            .expect("failed to write blob")
            .into()
    }

    fn build_tree(&self, files: &[(&str, &[u8])]) -> super::Oid {
        // Start from the empty tree, then apply updates for each file.
        let empty_tree = {
            let oid = self
                .inner
                .treebuilder(None)
                .expect("treebuilder")
                .write()
                .expect("write empty tree");
            self.inner.find_tree(oid).expect("find empty tree")
        };

        let mut builder = super::build::TreeUpdateBuilder::new();
        for (path, content) in files {
            let blob_oid = self.inner.blob(content).expect("failed to write blob");
            builder.upsert(path, blob_oid, super::FileMode::Blob);
        }

        builder
            .create_updated(&self.inner, &empty_tree)
            .expect("failed to build tree")
    }
}
