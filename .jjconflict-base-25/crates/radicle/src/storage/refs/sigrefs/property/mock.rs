use qcheck::{Arbitrary, Gen};
use radicle_core::RepoId;
use radicle_git_metadata::author::{Author, Time};
use radicle_oid::Oid;
use tempfile::TempDir;

use crate::identity::doc;
use crate::storage::refs::sigrefs::git::Committer;
use crate::storage::refs::{IDENTITY_ROOT, Refs};

/// A `Vec<T>` whose [`Arbitrary`] instance caps the length at
/// [`Self::MAX_LEN`], preventing the property runner from generating inputs
/// that would make the test prohibitively slow.
#[derive(Clone, Debug)]
pub struct BoundedVec<T>(pub Vec<T>);

impl<T> BoundedVec<T> {
    const MAX_LEN: usize = 16;
}

impl<T: Arbitrary> Arbitrary for BoundedVec<T> {
    fn arbitrary(g: &mut Gen) -> Self {
        let len = usize::arbitrary(g) % (Self::MAX_LEN + 1);
        BoundedVec((0..len).map(|_| T::arbitrary(g)).collect())
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let inner: Vec<Vec<T>> = self.0.shrink().collect();
        Box::new(inner.into_iter().map(BoundedVec))
    }
}

/// A Radicle Git repository fixture.
///
/// It is initialized in a [`TempDir`], and starts off with a single blob to
/// emulate the identity document.
pub struct Fixture {
    /// The underlying Git repository.
    repo: git2::Repository,
    /// The [`RepoId`] of the initial identity document blob.
    rid: RepoId,
    /// The commit that points to the identity document, which provides the
    /// [`RepoId`].
    identity_commit: Oid,
    _dir: TempDir,
}

impl Fixture {
    /// Initialise a bare git repository and write the minimal object graph
    /// required for identity-root verification to succeed:
    ///
    /// ```text
    /// identity-commit
    ///   └─ tree
    ///        └─ embeds/
    ///             └─ <doc::PATH>  (blob whose OID becomes the RepoId)
    /// ```
    pub fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let repo = git2::Repository::init_bare(dir.path()).unwrap();

        let (identity_commit, rid) = Self::identity_commit(&repo);

        Self {
            _dir: dir,
            repo,
            rid,
            identity_commit,
        }
    }

    /// Return the [`RepoId`] of the fixture.
    pub fn rid(&self) -> RepoId {
        self.rid
    }

    /// Return the underlying Git repository of the fixture.
    pub fn repo(&self) -> &git2::Repository {
        &self.repo
    }

    /// Return a [`Committer`] with a fixed, stable [`Author`].
    pub fn committer(&self) -> Committer {
        Committer::new(Author {
            name: "radicle".to_string(),
            email: "radicle@test".to_string(),
            time: Time::new(0, 0),
        })
    }

    /// Patch an arbitrary [`Refs`] so that its [`IDENTITY_ROOT`] entry
    /// points at the fixture's identity commit, satisfying the
    /// `debug_assert` in [`SignedRefsWriter::new`] and the identity
    /// verification in [`crate::storage::refs::sigrefs::read`].
    pub fn with_identity_root(&self, mut refs: Refs) -> Refs {
        refs.insert(IDENTITY_ROOT.to_ref_string(), self.identity_commit);
        refs
    }

    fn identity_commit(repo: &git2::Repository) -> (Oid, RepoId) {
        let (doc_tree, rid) = Self::write_doc_blob(repo);
        let tree = Self::write_embeds(repo, doc_tree);
        let tree = repo.find_tree(tree).unwrap();
        let sig = git2::Signature::new("radicle", "radicle@test", &git2::Time::new(0, 0)).unwrap();
        let oid = repo
            .commit(None, &sig, &sig, "identity root", &tree, &[])
            .unwrap();
        (oid.into(), rid)
    }

    fn write_doc_blob(repo: &git2::Repository) -> (git2::Oid, RepoId) {
        let doc_blob_oid = repo.blob(b"identity").unwrap();
        let rid = RepoId::from(Oid::from(doc_blob_oid));

        let mut tb = repo.treebuilder(None).unwrap();
        tb.insert(
            doc::PATH.as_os_str(),
            doc_blob_oid,
            git2::FileMode::Blob.into(),
        )
        .unwrap();
        let oid = tb.write().unwrap();
        (oid, rid)
    }

    fn write_embeds(repo: &git2::Repository, doc: git2::Oid) -> git2::Oid {
        let mut tb = repo.treebuilder(None).unwrap();
        tb.insert("embeds", doc, git2::FileMode::Tree.into())
            .unwrap();
        tb.write().unwrap()
    }
}
