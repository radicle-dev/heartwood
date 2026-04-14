//! Mock implementations of [`ObjectReader`] and [`reference::Reader`] for
//! unit-testing.
//!
//! [`ObjectReader`]: crate::git::repository::ObjectReader

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use radicle_core::NodeId;
use radicle_git_metadata::author::{Author, Time};
use radicle_git_metadata::commit::CommitData;
use radicle_git_metadata::commit::headers::Headers;
use radicle_git_metadata::commit::trailers::OwnedTrailer;
use radicle_oid::Oid;

use crate::git;
use crate::git::repository::object;
use crate::git::repository::reference;
use crate::git::repository::types::{Blob, Commit};
use crate::identity::doc;
use crate::storage::refs::{REFS_BLOB_PATH, Refs, SIGNATURE_BLOB_PATH, SIGREFS_BRANCH};

pub(crate) const MOCKED_IDENTITY: u8 = 99u8;

/// A configurable in-memory repository implementing [`ObjectReader`] and
/// [`reference::Reader`].
/// All behaviour is set at construction time via the builder methods; the mock
/// is fully deterministic.
pub struct MockRepository {
    commits: HashMap<Oid, CommitBehavior>,
    blobs: HashMap<(Oid, PathBuf), BlobBehavior>,
    references: HashMap<String, RefBehavior>,
}

enum CommitBehavior {
    /// [`ObjectReader::commit`] returns `Ok(Some(commit))`.
    Present(Box<CommitData<Oid, Oid>>),
    /// [`ObjectReader::commit`] returns `Ok(None)`.
    Missing,
    /// [`ObjectReader::commit`] returns `Err(…)`.
    Error,
}

enum BlobBehavior {
    /// [`ObjectReader::blob_at`] returns `Ok(Some(blob))`.
    Present(Vec<u8>),
    /// [`ObjectReader::blob_at`] returns `Ok(None)`.
    Missing,
    /// [`ObjectReader::blob_at`] returns `Err(…)`.
    Error,
}

enum RefBehavior {
    /// [`reference::Reader::find_reference`] returns `Ok(Some(oid))`.
    Present(Oid),
    /// [`reference::Reader::find_reference`] returns `Ok(None)`.
    Missing,
    /// [`reference::Reader::find_reference`] returns `Err(…)`.
    Error,
}

impl MockRepository {
    pub fn new() -> Self {
        Self {
            commits: HashMap::new(),
            blobs: HashMap::new(),
            references: HashMap::new(),
        }
        .with_identity(oid(MOCKED_IDENTITY))
    }

    pub fn with_commit(mut self, oid: Oid, data: CommitData<Oid, Oid>) -> Self {
        self.commits
            .insert(oid, CommitBehavior::Present(Box::new(data)));
        self
    }

    pub fn with_missing_commit(mut self, oid: Oid) -> Self {
        self.commits.insert(oid, CommitBehavior::Missing);
        self
    }

    pub fn with_commit_error(mut self, oid: Oid) -> Self {
        self.commits.insert(oid, CommitBehavior::Error);
        self
    }

    pub fn with_refs(
        self,
        commit: Oid,
        refs: impl IntoIterator<Item = (git::fmt::RefString, Oid)>,
    ) -> Self {
        self.with_blob(commit, &REFS_BLOB_PATH, refs_bytes(refs))
    }

    pub fn with_signature(self, commit: Oid, id: u8) -> Self {
        self.with_blob(commit, &SIGNATURE_BLOB_PATH, sig_bytes(id))
    }

    pub fn with_blob<P>(mut self, commit: Oid, path: &P, bytes: Vec<u8>) -> Self
    where
        P: AsRef<Path>,
    {
        self.blobs.insert(
            (commit, path.as_ref().to_path_buf()),
            BlobBehavior::Present(bytes),
        );
        self
    }

    pub fn with_missing_refs(self, commit: Oid) -> Self {
        self.with_missing_blob(commit, &REFS_BLOB_PATH)
    }

    pub fn with_missing_signature(self, commit: Oid) -> Self {
        self.with_missing_blob(commit, &SIGNATURE_BLOB_PATH)
    }

    pub fn with_missing_identity(self, commit: Oid) -> Self {
        self.with_missing_blob(commit, &identity_path())
    }

    pub fn with_identity_error(self, commit: Oid) -> Self {
        self.with_blob_error(commit, &identity_path())
    }

    pub fn with_identity(self, commit: Oid) -> Self {
        self.with_blob(commit, &identity_path(), vec![])
    }

    fn with_missing_blob<P>(mut self, commit: Oid, path: &P) -> Self
    where
        P: AsRef<Path>,
    {
        self.blobs
            .insert((commit, path.as_ref().to_path_buf()), BlobBehavior::Missing);
        self
    }

    pub fn with_blob_error<P>(mut self, commit: Oid, path: &P) -> Self
    where
        P: AsRef<Path>,
    {
        self.blobs
            .insert((commit, path.as_ref().to_path_buf()), BlobBehavior::Error);
        self
    }

    /// The `name` must be the exact string returned by `Namespaced::as_str()`.
    pub fn with_rad_sigrefs(mut self, namespace: &NodeId, oid: Oid) -> Self {
        self.references.insert(
            sigrefs_ref_name(namespace).to_string(),
            RefBehavior::Present(oid),
        );
        self
    }

    pub fn with_missing_rad_sigrefs(mut self, namespace: &NodeId) -> Self {
        self.references
            .insert(sigrefs_ref_name(namespace), RefBehavior::Missing);
        self
    }

    pub fn with_rad_sigrefs_error(mut self, namespace: &NodeId) -> Self {
        self.references
            .insert(sigrefs_ref_name(namespace), RefBehavior::Error);
        self
    }
}

impl object::Reader for MockRepository {
    fn blob(&self, _oid: Oid) -> Result<Option<Blob>, object::error::read::Blob> {
        unimplemented!("MockRepository::blob")
    }

    fn blob_at<P: AsRef<Path>>(
        &self,
        commit: Oid,
        path: &P,
    ) -> Result<Option<Blob>, object::error::read::BlobAt> {
        let key = (commit, path.as_ref().to_path_buf());
        match self.blobs.get(&key) {
            Some(BlobBehavior::Present(bytes)) => Ok(Some(Blob {
                // The blob OID is returned as the commit OID.  This is
                // intentional: IdentityRootReader converts blob.oid into a
                // RepoId, so callers can predict which RepoId results from a
                // given identity-root commit OID.
                oid: commit,
                content: bytes.clone(),
            })),
            Some(BlobBehavior::Missing) | None => Ok(None),
            Some(BlobBehavior::Error) => Err(object::error::read::BlobAt::backend(
                std::io::Error::other("mock blob error"),
            )),
        }
    }

    fn commit(&self, oid: Oid) -> Result<Option<Commit>, object::error::read::Commit> {
        match self.commits.get(&oid) {
            Some(CommitBehavior::Present(data)) => {
                let bytes = data.to_string();
                let parsed = Commit::from_bytes(bytes.as_bytes())
                    .map_err(|e| object::error::read::Commit::Parse { oid, source: e })?;
                Ok(Some(parsed))
            }
            Some(CommitBehavior::Missing) | None => Ok(None),
            Some(CommitBehavior::Error) => Err(object::error::read::Commit::backend(
                std::io::Error::other("mock commit error"),
            )),
        }
    }

    fn exists(&self, _oid: Oid) -> Result<bool, object::error::read::Exists> {
        unimplemented!("MockRepository::exists")
    }

    fn object_kind(
        &self,
        _oid: Oid,
    ) -> Result<Option<crate::git::repository::ObjectKind>, object::error::read::ObjectKind> {
        unimplemented!("MockRepository::object_kind")
    }
}

impl reference::Reader for MockRepository {
    type References<'a> = std::iter::Empty<
        Result<(git::fmt::Qualified<'static>, Oid), reference::error::read::ListReference>,
    >;

    fn ref_target<R: AsRef<git::fmt::RefStr>>(
        &self,
        name: &R,
    ) -> Result<Option<Oid>, reference::error::read::RefTarget> {
        match self.references.get(name.as_ref().as_str()) {
            Some(RefBehavior::Present(oid)) => Ok(Some(*oid)),
            Some(RefBehavior::Missing) | None => Ok(None),
            Some(RefBehavior::Error) => Err(reference::error::read::RefTarget::backend(
                std::io::Error::other("mock reference error"),
            )),
        }
    }

    fn list_refs<'a, P: AsRef<git::fmt::refspec::PatternStr>>(
        &'a self,
        _pattern: &P,
    ) -> Result<Self::References<'a>, reference::error::read::ListRefs> {
        unimplemented!("MockRepository::list_refs")
    }
}

/// Accepts every (message, signature) pair without inspecting either.
pub struct AlwaysVerify;

impl crypto::signature::Verifier<crypto::Signature> for AlwaysVerify {
    fn verify(
        &self,
        _msg: &[u8],
        _sig: &crypto::Signature,
    ) -> Result<(), crypto::signature::Error> {
        Ok(())
    }
}

/// Rejects every (message, signature) pair.
pub struct NeverVerify;

impl crypto::signature::Verifier<crypto::Signature> for NeverVerify {
    fn verify(
        &self,
        _msg: &[u8],
        _sig: &crypto::Signature,
    ) -> Result<(), crypto::signature::Error> {
        Err(crypto::signature::Error::new())
    }
}

/// Construct an [`Oid`] from a single repeated byte.
///
/// `oid(1) != oid(2)` is guaranteed; use distinct values for distinct objects.
pub fn oid(n: u8) -> Oid {
    Oid::from_sha1([n; 20])
}

/// Construct a [`radicle_core::RepoId`] from a single repeated byte.
pub fn rid(n: u8) -> radicle_core::RepoId {
    radicle_core::RepoId::from(oid(n))
}

/// Construct a [`radicle_core::NodeId`] from a single repeated byte.
pub fn node_id() -> NodeId {
    NodeId::from([1u8; 32])
}

pub fn refs_heads_main() -> git::fmt::RefString {
    git::fmt::refname!("refs/heads/main")
}

/// Compute the namespaced sigrefs reference string for `namespace`, matching
/// the string that `SignedRefsReader::resolve_tip` will look up.
fn sigrefs_ref_name(namespace: &NodeId) -> String {
    SIGREFS_BRANCH
        .with_namespace(git::fmt::Component::from(namespace))
        .as_str()
        .to_owned()
}

fn test_author() -> Author {
    Author {
        name: "test".to_owned(),
        email: "test@example.com".to_owned(),
        time: Time::new(0, 0),
    }
}

/// Build a minimal [`CommitData`] with the given parents and a zero tree OID.
pub fn commit_data(parents: impl IntoIterator<Item = Oid>) -> CommitData<Oid, Oid> {
    let tree = oid(0);
    let author = test_author();
    let message = "test\n".to_owned();

    CommitData::new::<_, _, OwnedTrailer>(
        tree,
        parents,
        author.clone(),
        author,
        Headers::new(),
        message,
        vec![],
    )
}

/// Returns 64 bytes all equal to `id`.
///
/// With [`AlwaysVerify`] any 64-byte sequence is accepted as a valid signature.
/// Different `id` values are treated as distinct signatures by the
/// deduplication logic inside [`SignedRefsReader`].
pub fn sig_bytes(id: u8) -> Vec<u8> {
    vec![id; 64]
}

/// Set up a linear commit chain in the mock repository.
///
/// `chain` is ordered oldest-first: `chain[0]` is the root (no parent),
/// and each subsequent commit's parent is the preceding entry.
/// Each element is `(commit_oid, sig_id, refs)`.
pub fn setup_chain<I>(chain: impl IntoIterator<Item = (Oid, u8, I)>) -> MockRepository
where
    I: IntoIterator<Item = (git::fmt::RefString, Oid)>,
{
    let mut repo = MockRepository::new();
    let mut parent = None;
    for (commit_oid, sig_id, refs) in chain.into_iter() {
        repo = repo
            .with_commit(commit_oid, commit_data(parent))
            .with_refs(commit_oid, refs)
            .with_signature(commit_oid, sig_id);
        parent = Some(commit_oid);
    }
    repo
}

/// Construct the canonical bytes of a [`Refs`] from the given entries.
fn refs_bytes(entries: impl IntoIterator<Item = (git::fmt::RefString, Oid)>) -> Vec<u8> {
    let refs = Refs::from(entries.into_iter());
    refs.canonical()
}

fn identity_path() -> PathBuf {
    Path::new("embeds").join(*doc::PATH)
}
