use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr as _;

use radicle_core::{NodeId, RepoId};
use radicle_git_metadata::author::{Author, Time};
use radicle_git_metadata::commit::CommitData;
use radicle_git_metadata::commit::headers::Headers;
use radicle_git_metadata::commit::trailers::OwnedTrailer;
use radicle_oid::Oid;

use crate::git;
use crate::identity::doc;
use crate::storage::HasRepoId;
use crate::storage::refs::sigrefs::git::{object, reference};
use crate::storage::refs::{REFS_BLOB_PATH, Refs, SIGNATURE_BLOB_PATH, SIGREFS_BRANCH};

const MOCKED_IDENTITY: u8 = 99u8;

enum WriteTreeBehavior {
    /// [`object::Writer::write_tree`] returns `Ok(oid)`.
    Ok(Oid),
    /// [`object::Writer::write_tree`] returns `Err(…)`.
    Error,
}

/// [`object::Writer::write_commit`] returns `Ok(oid)`.
struct WriteCommitBehavior(Oid);

enum WriteReferenceBehavior {
    /// [`reference::Writer::write_reference`] returns `Ok(())`.
    Ok,
    /// [`reference::Writer::write_reference`] returns `Err(…)`.
    Error,
}

enum BlobBehavior {
    /// [`object::Reader::read_blob`] returns `Ok(Some(blob))`.
    Present(Vec<u8>),
    /// [`object::Reader::read_blob`] returns `Ok(None)`.
    Missing,
    /// [`object::Reader::read_blob`] returns `Err(…)`.
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

pub struct MockRepository {
    commits: HashMap<Oid, CommitBehavior>,
    blobs: HashMap<(Oid, PathBuf), BlobBehavior>,
    references: HashMap<String, RefBehavior>,
    write_tree: Option<WriteTreeBehavior>,
    write_commit: Option<WriteCommitBehavior>,
    write_reference: Option<WriteReferenceBehavior>,
}

enum CommitBehavior {
    /// [`object::Reader::read_commit`] returns `Ok(Some(bytes))`.
    Present(Box<CommitData<Oid, Oid>>),
}

impl MockRepository {
    pub fn new() -> MockRepository {
        MockRepository {
            commits: HashMap::new(),
            blobs: HashMap::new(),
            references: HashMap::new(),
            write_tree: None,
            write_commit: None,
            write_reference: None,
        }
        .with_identity(oid(MOCKED_IDENTITY))
    }

    pub fn with_commit(mut self, oid: Oid, data: CommitData<Oid, Oid>) -> Self {
        self.commits
            .insert(oid, CommitBehavior::Present(Box::new(data)));
        self
    }

    pub fn with_rad_sigrefs(mut self, namespace: &NodeId, commit: Oid) -> MockRepository {
        self.references
            .insert(sigrefs_ref_name(namespace), RefBehavior::Present(commit));
        self
    }

    pub fn with_missing_rad_sigrefs(mut self, namespace: &NodeId) -> MockRepository {
        self.references
            .insert(sigrefs_ref_name(namespace), RefBehavior::Missing);
        self
    }

    pub fn with_rad_sigrefs_error(mut self, namespace: &NodeId) -> MockRepository {
        self.references
            .insert(sigrefs_ref_name(namespace), RefBehavior::Error);
        self
    }

    pub fn with_refs(
        self,
        commit: Oid,
        refs: impl IntoIterator<Item = (git::fmt::RefString, Oid)>,
    ) -> MockRepository {
        let refs = Refs::from(refs.into_iter());
        self.with_blob(commit, Path::new(REFS_BLOB_PATH), refs.canonical())
    }

    pub fn with_refs_error(self, commit: Oid) -> MockRepository {
        self.with_blob_error(commit, Path::new(REFS_BLOB_PATH))
    }

    pub fn with_missing_refs(self, commit: Oid) -> MockRepository {
        self.with_missing_blob(commit, Path::new(REFS_BLOB_PATH))
    }

    pub fn with_invalid_refs(self, commit: Oid) -> MockRepository {
        self.with_blob(
            commit,
            Path::new(REFS_BLOB_PATH),
            b"NOT VALID REFS\n".to_vec(),
        )
    }

    pub fn with_signature(self, commit: Oid, sig_id: u8) -> MockRepository {
        self.with_blob(commit, Path::new(SIGNATURE_BLOB_PATH), vec![sig_id; 64])
    }

    pub fn with_signature_error(self, commit: Oid) -> MockRepository {
        self.with_blob_error(commit, Path::new(SIGNATURE_BLOB_PATH))
    }

    pub fn with_missing_signature(self, commit: Oid) -> MockRepository {
        self.with_missing_blob(commit, Path::new(SIGNATURE_BLOB_PATH))
    }

    pub fn with_invalid_signature(self, commit: Oid) -> MockRepository {
        let bytes = vec![0u8; 1];
        assert!(crypto::Signature::from_str(std::str::from_utf8(&bytes).unwrap()).is_err());
        self.with_blob(commit, Path::new(SIGNATURE_BLOB_PATH), bytes)
    }

    pub fn with_identity(self, commit: Oid) -> Self {
        self.with_blob(commit, &identity_path(), vec![])
    }

    fn with_blob(mut self, commit: Oid, path: &Path, bytes: Vec<u8>) -> Self {
        self.blobs
            .insert((commit, path.to_path_buf()), BlobBehavior::Present(bytes));
        self
    }

    fn with_blob_error(mut self, commit: Oid, path: &Path) -> Self {
        self.blobs
            .insert((commit, path.to_path_buf()), BlobBehavior::Error);
        self
    }

    fn with_missing_blob(mut self, commit: Oid, path: &Path) -> Self {
        self.blobs
            .insert((commit, path.to_path_buf()), BlobBehavior::Missing);
        self
    }

    pub fn with_write_tree_error(mut self) -> Self {
        self.write_tree = Some(WriteTreeBehavior::Error);
        self
    }

    pub fn with_write_tree_ok(mut self, oid: Oid) -> Self {
        self.write_tree = Some(WriteTreeBehavior::Ok(oid));
        self
    }

    pub fn with_write_commit_ok(mut self, oid: Oid) -> Self {
        self.write_commit = Some(WriteCommitBehavior(oid));
        self
    }

    pub fn with_write_reference_ok(mut self) -> Self {
        self.write_reference = Some(WriteReferenceBehavior::Ok);
        self
    }

    pub fn with_write_reference_error(mut self) -> Self {
        self.write_reference = Some(WriteReferenceBehavior::Error);
        self
    }
}

impl HasRepoId for MockRepository {
    fn rid(&self) -> radicle_core::RepoId {
        rid()
    }
}

impl object::Reader for MockRepository {
    fn read_commit(&self, oid: &Oid) -> Result<Option<Vec<u8>>, object::error::ReadCommit> {
        match self.commits.get(oid) {
            Some(CommitBehavior::Present(data)) => Ok(Some(data.to_string().as_bytes().to_vec())),
            None => Ok(None),
        }
    }

    fn read_blob(
        &self,
        commit: &Oid,
        path: &Path,
    ) -> Result<Option<object::Blob>, object::error::ReadBlob> {
        let key = (*commit, path.to_path_buf());
        match self.blobs.get(&key) {
            Some(BlobBehavior::Present(bytes)) => Ok(Some(object::Blob {
                oid: *commit,
                bytes: bytes.clone(),
            })),
            Some(BlobBehavior::Missing) | None => Ok(None),
            Some(BlobBehavior::Error) => Err(object::error::ReadBlob::other(
                std::io::Error::other("mock blob error"),
            )),
        }
    }
}

impl reference::Reader for MockRepository {
    fn find_reference(
        &self,
        reference: &git::fmt::Namespaced,
    ) -> Result<Option<Oid>, reference::error::FindReference> {
        match self.references.get(reference.as_str()) {
            Some(RefBehavior::Present(oid)) => Ok(Some(*oid)),
            Some(RefBehavior::Missing) | None => Ok(None),
            Some(RefBehavior::Error) => Err(reference::error::FindReference::other(
                std::io::Error::other("mock reference error"),
            )),
        }
    }
}

impl object::Writer for MockRepository {
    fn write_tree(
        &self,
        _refs: object::RefsEntry,
        _signature: object::SignatureEntry,
    ) -> Result<Oid, object::error::WriteTree> {
        match &self.write_tree {
            Some(WriteTreeBehavior::Ok(oid)) => Ok(*oid),
            Some(WriteTreeBehavior::Error) | None => Err(object::error::WriteTree::write_error(
                std::io::Error::other("mock write_tree error"),
            )),
        }
    }

    fn write_commit(&self, _bytes: &[u8]) -> Result<Oid, object::error::WriteCommit> {
        match &self.write_commit {
            Some(WriteCommitBehavior(oid)) => Ok(*oid),
            None => Err(object::error::WriteCommit::other(std::io::Error::other(
                "mock write_commit error",
            ))),
        }
    }
}

impl reference::Writer for MockRepository {
    fn write_reference(
        &self,
        _reference: &git::fmt::Namespaced,
        _commit: Oid,
        _parent: Option<Oid>,
        _reflog: String,
    ) -> Result<(), reference::error::WriteReference> {
        match &self.write_reference {
            Some(WriteReferenceBehavior::Ok) => Ok(()),
            Some(WriteReferenceBehavior::Error) | None => {
                Err(reference::error::WriteReference::other(
                    std::io::Error::other("mock write_reference error"),
                ))
            }
        }
    }
}

/// Always signs successfully, returning a fixed 64-byte signature.
pub struct AlwaysSign;

impl AlwaysSign {
    const SIGNATURE: [u8; 64] = [1u8; 64];

    pub fn signature() -> crypto::Signature {
        crypto::Signature::from(Self::SIGNATURE)
    }
}

impl crypto::signature::Signer<crypto::Signature> for AlwaysSign {
    fn try_sign(&self, _msg: &[u8]) -> Result<crypto::Signature, crypto::signature::Error> {
        Ok(Self::signature())
    }
}

impl crypto::signature::Verifier<crypto::Signature> for AlwaysSign {
    fn verify(
        &self,
        _msg: &[u8],
        _sig: &crypto::Signature,
    ) -> Result<(), crypto::signature::Error> {
        Ok(())
    }
}

/// Always fails to sign.
pub struct NeverSign;

impl crypto::signature::Signer<crypto::Signature> for NeverSign {
    fn try_sign(&self, _msg: &[u8]) -> Result<crypto::Signature, crypto::signature::Error> {
        Err(crypto::signature::Error::new())
    }
}

/// Construct an [`Oid`] from a single repeated byte.
///
/// `oid(1) != oid(2)` is guaranteed; use distinct values for distinct objects.
pub fn oid(n: u8) -> Oid {
    Oid::from_sha1([n; 20])
}

/// A fixed [`RepoId`] for tests.
pub fn rid() -> RepoId {
    RepoId::from(oid(MOCKED_IDENTITY))
}

/// A fixed [`NodeId`] for tests.
pub fn node_id() -> NodeId {
    NodeId::from([1u8; 32])
}

pub fn refs_heads_main() -> git::fmt::RefString {
    git::fmt::refname!("refs/heads/main")
}

/// A minimal [`radicle_git_metadata::author::Author`] for use in tests.
pub fn author() -> Author {
    Author {
        name: "test".to_owned(),
        email: "test@example.com".to_owned(),
        time: Time::new(0, 0),
    }
}

pub fn commit_data(parents: impl IntoIterator<Item = Oid>) -> CommitData<Oid, Oid> {
    let tree = oid(0);
    let author = author();
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

fn sigrefs_ref_name(namespace: &NodeId) -> String {
    SIGREFS_BRANCH
        .with_namespace(git::fmt::Component::from(namespace))
        .as_str()
        .to_owned()
}

fn identity_path() -> PathBuf {
    Path::new("embeds").join(*doc::PATH)
}
