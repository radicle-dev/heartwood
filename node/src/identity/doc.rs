use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io;
use std::marker::PhantomData;
use std::ops::Deref;
use std::path::Path;

use nonempty::NonEmpty;
use once_cell::sync::Lazy;
use radicle_git_ext::Oid;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto;
use crate::crypto::{Signature, Unverified, Verified};
use crate::git;
use crate::identity::{Did, Id};
use crate::storage::git::trailers;
use crate::storage::{BranchName, ReadRepository, RemoteId, WriteRepository, WriteStorage};

pub use crypto::PublicKey;

/// Untrusted, well-formed input.
#[derive(Clone, Copy, Debug)]
pub struct Untrusted;
/// Signed by quorum of the previous delegation.
#[derive(Clone, Copy, Debug)]
pub struct Trusted;

pub static REFERENCE_NAME: Lazy<git::RefString> = Lazy::new(|| git::refname!("heads/radicle/id"));
pub static PATH: Lazy<&Path> = Lazy::new(|| Path::new("radicle.json"));

pub const MAX_STRING_LENGTH: usize = 255;
pub const MAX_DELEGATES: usize = 255;

#[derive(Error, Debug)]
pub enum Error {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("verification: {0}")]
    Verification(#[from] VerificationError),
    #[error("git: {0}")]
    Git(#[from] git::Error),
    #[error("git: {0}")]
    RawGit(#[from] git2::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Delegate {
    pub name: String,
    pub id: Did,
}

impl Delegate {
    fn matches(&self, key: &PublicKey) -> bool {
        &self.id.0 == key
    }
}

impl From<Delegate> for PublicKey {
    fn from(delegate: Delegate) -> Self {
        delegate.id.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Payload {
    pub name: String,
    pub description: String,    // TODO: Make optional.
    pub default_branch: String, // TODO: Make optional.
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
// TODO: Restrict values.
pub struct Namespace(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Doc<V> {
    #[serde(rename = "xyz.radicle.project")]
    pub payload: Payload,
    #[serde(flatten)]
    pub extensions: BTreeMap<Namespace, serde_json::Value>,
    pub delegates: NonEmpty<Delegate>,
    pub threshold: usize,

    verified: PhantomData<V>,
}

impl Doc<Verified> {
    pub fn encode(&self) -> Result<(git::Oid, Vec<u8>), Error> {
        let mut buf = Vec::new();
        let mut serializer =
            serde_json::Serializer::with_formatter(&mut buf, olpc_cjson::CanonicalFormatter::new());

        self.serialize(&mut serializer)?;
        let oid = git2::Oid::hash_object(git2::ObjectType::Blob, &buf)?;

        Ok((oid.into(), buf))
    }

    /// Attempt to add a new delegate to the document. Returns `true` if it wasn't there before.
    pub fn delegate(&mut self, delegate: Delegate) -> bool {
        if self.delegates.iter().all(|d| d.id != delegate.id) {
            self.delegates.push(delegate);
            return true;
        }
        false
    }

    pub fn sign<G: crypto::Signer>(&self, signer: G) -> Result<(git::Oid, Signature), Error> {
        let (oid, bytes) = self.encode()?;
        let sig = signer.sign(&bytes);

        Ok((oid, sig))
    }

    pub fn create<'r, S: WriteStorage<'r>>(
        &self,
        remote: &RemoteId,
        msg: &str,
        storage: &'r S,
    ) -> Result<(Id, git::Oid, S::Repository), Error> {
        // You can checkout this branch in your working copy with:
        //
        //      git fetch rad
        //      git checkout -b radicle/id remotes/rad/radicle/id
        //
        let (doc_oid, doc) = self.encode()?;
        let id = Id::from(doc_oid);
        let repo = storage.repository(&id).unwrap();
        let tree = git::write_tree(*PATH, doc.as_slice(), repo.raw())?;
        let oid = Doc::commit(remote, &tree, msg, &[], repo.raw())?;

        drop(tree);

        Ok((id, oid, repo))
    }

    pub fn update<'r, R: WriteRepository<'r>>(
        &self,
        remote: &RemoteId,
        msg: &str,
        signatures: &[(&PublicKey, Signature)],
        repo: &R,
    ) -> Result<git::Oid, Error> {
        let mut msg = format!("{msg}\n\n");
        for (key, sig) in signatures {
            writeln!(&mut msg, "{}: {key} {sig}", trailers::SIGNATURE_TRAILER)
                .expect("in-memory writes don't fail");
        }

        let (_, doc) = self.encode()?;
        let tree = git::write_tree(*PATH, doc.as_slice(), repo.raw())?;
        let id_ref = format!("refs/remotes/{remote}/{}", &*REFERENCE_NAME);
        let head = repo.raw().find_reference(&id_ref)?.peel_to_commit()?;
        let oid = Doc::commit(remote, &tree, &msg, &[&head], repo.raw())?;

        Ok(oid)
    }

    fn commit(
        remote: &RemoteId,
        tree: &git2::Tree,
        msg: &str,
        parents: &[&git2::Commit],
        repo: &git2::Repository,
    ) -> Result<git::Oid, Error> {
        let sig = repo
            .signature()
            .or_else(|_| git2::Signature::now("radicle", remote.to_string().as_str()))?;

        let id_ref = format!("refs/remotes/{remote}/{}", &*REFERENCE_NAME);
        let oid = repo.commit(Some(&id_ref), &sig, &sig, msg, tree, parents)?;

        Ok(oid.into())
    }
}

impl<V> Deref for Doc<V> {
    type Target = Payload;

    fn deref(&self) -> &Self::Target {
        &self.payload
    }
}

#[derive(Error, Debug)]
pub enum VerificationError {
    #[error("invalid name: {0}")]
    Name(&'static str),
    #[error("invalid description: {0}")]
    Description(&'static str),
    #[error("invalid default branch: {0}")]
    DefaultBranch(&'static str),
    #[error("invalid delegates: {0}")]
    Delegates(&'static str),
    #[error("invalid version `{0}`")]
    Version(u32),
    #[error("invalid parent: {0}")]
    Parent(&'static str),
    #[error("invalid threshold `{0}`: {1}")]
    Threshold(usize, &'static str),
}

impl Doc<Unverified> {
    pub fn initial(
        name: String,
        description: String,
        default_branch: BranchName,
        delegate: Delegate,
    ) -> Self {
        Self {
            payload: Payload {
                name,
                description,
                default_branch,
            },
            extensions: BTreeMap::new(),
            delegates: NonEmpty::new(delegate),
            threshold: 1,
            verified: PhantomData,
        }
    }

    pub fn new(
        name: String,
        description: String,
        default_branch: BranchName,
        delegates: NonEmpty<Delegate>,
        threshold: usize,
    ) -> Self {
        Self {
            payload: Payload {
                name,
                description,
                default_branch,
            },
            extensions: BTreeMap::new(),
            delegates,
            threshold,
            verified: PhantomData,
        }
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn verified(self) -> Result<Doc<Verified>, VerificationError> {
        if self.name.is_empty() {
            return Err(VerificationError::Name("name cannot be empty"));
        }
        if self.name.len() > MAX_STRING_LENGTH {
            return Err(VerificationError::Name("name cannot exceed 255 bytes"));
        }
        if self.description.len() > MAX_STRING_LENGTH {
            return Err(VerificationError::Description(
                "description cannot exceed 255 bytes",
            ));
        }
        if self.delegates.len() > MAX_DELEGATES {
            return Err(VerificationError::Delegates(
                "number of delegates cannot exceed 255",
            ));
        }
        if self
            .delegates
            .iter()
            .any(|d| d.name.is_empty() || d.name.len() > MAX_STRING_LENGTH)
        {
            return Err(VerificationError::Delegates(
                "delegate name must not be empty and must not exceed 255 bytes",
            ));
        }
        if self.delegates.is_empty() {
            return Err(VerificationError::Delegates(
                "delegate list cannot be empty",
            ));
        }
        if self.default_branch.is_empty() {
            return Err(VerificationError::DefaultBranch(
                "default branch cannot be empty",
            ));
        }
        if self.default_branch.len() > MAX_STRING_LENGTH {
            return Err(VerificationError::DefaultBranch(
                "default branch cannot exceed 255 bytes",
            ));
        }
        if self.threshold > self.delegates.len() {
            return Err(VerificationError::Threshold(
                self.threshold,
                "threshold cannot exceed number of delegates",
            ));
        }
        if self.threshold == 0 {
            return Err(VerificationError::Threshold(
                self.threshold,
                "threshold cannot be zero",
            ));
        }

        Ok(Doc {
            payload: self.payload,
            extensions: self.extensions,
            delegates: self.delegates,
            threshold: self.threshold,
            verified: PhantomData,
        })
    }

    pub fn blob_at<'r, R: ReadRepository<'r>>(
        commit: Oid,
        repo: &R,
    ) -> Result<Option<git2::Blob>, git::Error> {
        match repo.blob_at(commit, Path::new(&*PATH)) {
            Err(git::ext::Error::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
            Ok(blob) => Ok(Some(blob)),
        }
    }

    pub fn load_at<'r, R: ReadRepository<'r>>(
        commit: Oid,
        repo: &R,
    ) -> Result<Option<(Self, Oid)>, git::Error> {
        if let Some(blob) = Self::blob_at(commit, repo)? {
            let doc = Doc::from_json(blob.content()).unwrap();
            return Ok(Some((doc, blob.id().into())));
        }
        Ok(None)
    }

    pub fn load<'r, R: ReadRepository<'r>>(
        remote: &RemoteId,
        repo: &R,
    ) -> Result<Option<(Self, Oid)>, git::Error> {
        if let Some(oid) = Self::head(remote, repo)? {
            Self::load_at(oid, repo)
        } else {
            Ok(None)
        }
    }
}

impl<V> Doc<V> {
    pub fn head<'r, R: ReadRepository<'r>>(
        remote: &RemoteId,
        repo: &R,
    ) -> Result<Option<Oid>, git::Error> {
        if let Some(oid) = repo.reference_oid(remote, &REFERENCE_NAME)? {
            Ok(Some(oid))
        } else {
            Ok(None)
        }
    }
}

#[derive(Error, Debug)]
pub enum IdentityError {
    #[error("git: {0}")]
    GitRaw(#[from] git2::Error),
    #[error("git: {0}")]
    Git(#[from] git::Error),
    #[error("verification: {0}")]
    Verification(#[from] VerificationError),
    #[error("root hash `{0}` does not match project")]
    MismatchedRoot(Oid),
    #[error("commit signature for {0} is invalid: {1}")]
    InvalidSignature(PublicKey, crypto::Error),
    #[error("quorum not reached: {0} signatures for a threshold of {1}")]
    QuorumNotReached(usize, usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity<I> {
    /// The head of the identity branch. This points to a commit that
    /// contains the current document blob.
    pub head: Oid,
    /// The canonical identifier for this identity.
    /// This is the object id of the initial document blob.
    pub root: I,
    /// The object id of the current document blob.
    pub current: Oid,
    /// Revision number. The initial document has a revision of `0`.
    pub revision: u32,
    /// The current document.
    pub doc: Doc<Verified>,
    /// Signatures over this identity.
    pub signatures: HashMap<PublicKey, Signature>,
}

impl Identity<Oid> {
    pub fn verified(self, id: Id) -> Result<Identity<Id>, IdentityError> {
        // The root hash must be equal to the id.
        if self.root != *id {
            return Err(IdentityError::MismatchedRoot(self.root));
        }

        Ok(Identity {
            root: id,
            head: self.head,
            current: self.current,
            revision: self.revision,
            doc: self.doc,
            signatures: self.signatures,
        })
    }
}

impl Identity<Untrusted> {
    pub fn load<'r, R: ReadRepository<'r>>(
        remote: &RemoteId,
        repo: &R,
    ) -> Result<Option<Identity<Oid>>, IdentityError> {
        if let Some(head) = Doc::<Untrusted>::head(remote, repo)? {
            let mut history = repo.revwalk(head)?.collect::<Vec<_>>();

            // Retrieve root document.
            let root_oid = history.pop().unwrap()?.into();
            let root_blob = Doc::blob_at(root_oid, repo)?.unwrap();
            let root: git::Oid = root_blob.id().into();
            let trusted = Doc::from_json(root_blob.content()).unwrap();
            let revision = history.len() as u32;

            let mut trusted = trusted.verified()?;
            let mut current = root;
            let mut signatures = Vec::new();

            // Traverse the history chronologically.
            for oid in history.into_iter().rev() {
                let oid = oid?;
                let blob = Doc::blob_at(oid.into(), repo)?.unwrap();
                let untrusted = Doc::from_json(blob.content()).unwrap();
                let untrusted = untrusted.verified()?;
                let commit = repo.commit(oid.into())?.unwrap();
                let msg = commit.message_raw().unwrap();

                // Keys that signed the *current* document version.
                signatures = trailers::parse_signatures(msg).unwrap();
                for (pk, sig) in &signatures {
                    if let Err(err) = pk.verify(sig, blob.content()) {
                        return Err(IdentityError::InvalidSignature(*pk, err));
                    }
                }

                // Check that enough delegates signed this next version.
                let quorum = signatures
                    .iter()
                    .filter(|(key, _)| trusted.delegates.iter().any(|d| d.matches(key)))
                    .count();
                if quorum < trusted.threshold {
                    return Err(IdentityError::QuorumNotReached(quorum, trusted.threshold));
                }

                trusted = untrusted;
                current = blob.id().into();
            }

            return Ok(Some(Identity {
                root,
                head,
                current,
                revision,
                doc: trusted,
                signatures: signatures.into_iter().collect(),
            }));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod test {
    use crate::prelude::Signer;
    use crate::rad;
    use crate::storage::git::Storage;
    use crate::storage::{ReadStorage, WriteStorage};
    use crate::test::{crypto, fixtures};

    use super::*;
    use quickcheck_macros::quickcheck;

    #[test]
    fn test_valid_identity() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut rng = fastrand::Rng::new();

        let alice = crypto::MockSigner::new(&mut rng);
        let bob = crypto::MockSigner::new(&mut rng);
        let eve = crypto::MockSigner::new(&mut rng);

        let storage = Storage::open(tempdir.path().join("storage")).unwrap();
        let (id, _, _, _) =
            fixtures::project(tempdir.path().join("copy"), &storage, &alice).unwrap();

        // Bob and Eve fork the project from Alice.
        rad::fork(&id, alice.public_key(), &bob, &storage).unwrap();
        rad::fork(&id, alice.public_key(), &eve, &storage).unwrap();

        // TODO: In some cases we want to get the repo and the project, but don't
        // want to have to create a repository object twice. Perhaps there should
        // be a way of getting a project from a repo.
        let mut proj = storage.get(alice.public_key(), &id).unwrap().unwrap();
        let repo = storage.repository(&id).unwrap();

        // Make a change to the description and sign it.
        proj.doc.payload.description += "!";
        proj.sign(&alice)
            .and_then(|(_, sig)| {
                proj.update(
                    alice.public_key(),
                    "Update description",
                    &[(alice.public_key(), sig)],
                    &repo,
                )
            })
            .unwrap();

        // Add Bob as a delegate, and sign it.
        proj.delegate("bob".to_owned(), *bob.public_key());
        proj.doc.threshold = 2;
        proj.sign(&alice)
            .and_then(|(_, sig)| {
                proj.update(
                    alice.public_key(),
                    "Add bob",
                    &[(alice.public_key(), sig)],
                    &repo,
                )
            })
            .unwrap();

        // Add Eve as a delegate, and sign it.
        proj.delegate("eve".to_owned(), *eve.public_key());
        proj.sign(&alice)
            .and_then(|(_, alice_sig)| {
                proj.sign(&bob).and_then(|(_, bob_sig)| {
                    proj.update(
                        alice.public_key(),
                        "Add eve",
                        &[(alice.public_key(), alice_sig), (bob.public_key(), bob_sig)],
                        &repo,
                    )
                })
            })
            .unwrap();

        // Update description again with signatures by Eve and Bob.
        proj.doc.payload.description += "?";
        let (current, head) = proj
            .sign(&bob)
            .and_then(|(_, bob_sig)| {
                proj.sign(&eve).and_then(|(blob_id, eve_sig)| {
                    proj.update(
                        alice.public_key(),
                        "Update description",
                        &[(bob.public_key(), bob_sig), (eve.public_key(), eve_sig)],
                        &repo,
                    )
                    .map(|head| (blob_id, head))
                })
            })
            .unwrap();

        let identity: Identity<Id> = Identity::load(alice.public_key(), &repo)
            .unwrap()
            .unwrap()
            .verified(id.clone())
            .unwrap();

        assert_eq!(identity.signatures.len(), 2);
        assert_eq!(identity.revision, 4);
        assert_eq!(identity.root, id);
        assert_eq!(identity.current, current);
        assert_eq!(identity.head, head);
        assert_eq!(identity.doc, proj.doc);

        let proj = storage.get(alice.public_key(), &id).unwrap().unwrap();
        assert_eq!(proj.description, "Acme's repository!?");
    }

    #[quickcheck]
    fn prop_encode_decode(doc: Doc<Verified>) {
        let (_, bytes) = doc.encode().unwrap();
        assert_eq!(Doc::from_json(&bytes).unwrap().verified().unwrap(), doc);
    }
}
