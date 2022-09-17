use std::collections::HashMap;
use std::io;
use std::marker::PhantomData;
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
use crate::storage::{BranchName, ReadRepository, RemoteId};

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
pub struct Doc<V> {
    pub name: String,
    pub description: String,    // TODO: Make optional.
    pub default_branch: String, // TODO: Make optional.
    pub version: u32,           // TODO: Remove this.
    pub parent: Option<Oid>,
    pub delegates: NonEmpty<Delegate>,
    pub threshold: usize,

    verified: PhantomData<V>,
}

impl Doc<Verified> {
    pub fn encode(&self) -> Result<(Id, Vec<u8>), Error> {
        let mut buf = Vec::new();
        let mut serializer =
            serde_json::Serializer::with_formatter(&mut buf, olpc_cjson::CanonicalFormatter::new());

        self.serialize(&mut serializer)?;

        let oid = git2::Oid::hash_object(git2::ObjectType::Blob, &buf)?;
        let id = Id::from(oid);

        Ok((id, buf))
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
            name,
            description,
            default_branch,
            version: 1,
            parent: None,
            delegates: NonEmpty::new(delegate),
            threshold: 1,
            verified: PhantomData,
        }
    }

    pub fn new(
        name: String,
        description: String,
        default_branch: BranchName,
        parent: Option<Oid>,
        delegates: NonEmpty<Delegate>,
        threshold: usize,
    ) -> Self {
        Self {
            name,
            description,
            default_branch,
            version: 1,
            parent,
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
        if let Some(parent) = self.parent {
            if parent.is_zero() {
                return Err(VerificationError::Parent("parent cannot be zero"));
            }
        }
        if self.version != 1 {
            return Err(VerificationError::Version(self.version));
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
            name: self.name,
            description: self.description,
            delegates: self.delegates,
            default_branch: self.default_branch,
            parent: self.parent,
            version: self.version,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity<V> {
    /// The head of the identity branch. This points to a commit that
    /// contains the current document blob.
    pub head: Oid,
    /// The canonical identifier for this identity.
    /// This is the object id of the initial document blob.
    pub root: Id,
    /// The object id of the current document blob.
    pub current: Oid,
    /// The current document.
    pub doc: Doc<Verified>,
    /// Signatures over this identity.
    pub signatures: HashMap<PublicKey, Signature>,

    verified: PhantomData<V>,
}

impl Identity<Untrusted> {
    pub fn load<'r, R: ReadRepository<'r>>(
        id: &Id,
        remote: &RemoteId,
        repo: &R,
    ) -> Result<Option<Self>, Error> {
        if let Some(head) = Doc::<Untrusted>::head(remote, repo)? {
            let mut history = repo.revwalk(head)?.collect::<Vec<_>>();

            // Retrieve root document.
            let root_oid = history.pop().unwrap()?.into();
            let root_blob = Doc::blob_at(root_oid, repo)?.unwrap();
            let root = Id::from(root_blob.id());
            let trusted = Doc::from_json(root_blob.content()).unwrap();

            // The root hash must be equal to the id.
            if root != *id {
                todo!();
            }

            let mut trusted = trusted.verified()?;
            let mut current = *root;
            let mut signatures = Vec::new();

            // Traverse the history chronologically.
            for oid in history.into_iter().rev() {
                let oid = oid?;
                let blob = Doc::blob_at(head, repo)?.unwrap();
                let untrusted = Doc::from_json(blob.content()).unwrap();
                let untrusted = untrusted.verified()?;
                let commit = repo.commit(oid.into())?.unwrap();
                let msg = commit.message_raw().unwrap();

                // Keys that signed the *current* document version.
                signatures = trailers::parse_signatures(msg).unwrap();
                for (pk, sig) in &signatures {
                    if pk.verify(sig, blob.content()).is_err() {
                        todo!();
                    }
                }

                // Check that enough delegates signed this next version.
                let quorum = signatures
                    .iter()
                    .filter(|(key, _)| trusted.delegates.iter().any(|d| d.matches(key)))
                    .count();
                // TODO: Check that difference isn't greater than threshold?
                if quorum < trusted.threshold {
                    todo!();
                }

                trusted = untrusted;
                current = blob.id().into();
            }

            return Ok(Some(Self {
                root,
                head,
                current,
                doc: trusted,
                signatures: signatures.into_iter().collect(),
                verified: PhantomData,
            }));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_encode_decode(doc: Doc<Verified>) {
        let (_, bytes) = doc.encode().unwrap();
        assert_eq!(Doc::from_json(&bytes).unwrap().verified().unwrap(), doc);
    }
}
