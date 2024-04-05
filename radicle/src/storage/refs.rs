use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io;
use std::io::{BufRead, BufReader};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::str::FromStr;

use crypto::{PublicKey, Signature, Signer, SignerError, Unverified, Verified};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::git;
use crate::git::ext as git_ext;
use crate::git::Oid;
use crate::storage;
use crate::storage::{ReadRepository, RemoteId, WriteRepository};

pub use crate::git::refs::storage::*;

/// File in which the signed references are stored, in the `refs/rad/sigrefs` branch.
pub const REFS_BLOB_PATH: &str = "refs";
/// File in which the signature over the references is stored in the `refs/rad/sigrefs` branch.
pub const SIGNATURE_BLOB_PATH: &str = "signature";

#[derive(Debug)]
pub enum Updated {
    /// The computed [`Refs`] were stored as a new commit.
    Updated { oid: Oid },
    /// The stored [`Refs`] were the same as the computed ones, so no new commit
    /// was created.
    Unchanged { oid: Oid },
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid signature: {0}")]
    InvalidSignature(#[from] crypto::Error),
    #[error("signer error: {0}")]
    Signer(#[from] SignerError),
    #[error("canonical refs: {0}")]
    Canonical(#[from] canonical::Error),
    #[error("invalid reference")]
    InvalidRef,
    #[error("invalid reference: {0}")]
    Ref(#[from] git::RefError),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    GitExt(#[from] git_ext::Error),
}

impl Error {
    /// Whether this error is caused by a reference not being found.
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::GitExt(git::Error::NotFound(_)) => true,
            Self::GitExt(git::Error::Git(e)) if git::is_not_found_err(e) => true,
            Self::Git(e) if git::is_not_found_err(e) => true,
            _ => false,
        }
    }
}

// TODO(finto): we should turn `git::RefString` to `git::Qualified`,
// since all these refs SHOULD be `Qualified`.
/// The published state of a local repository.
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Refs(BTreeMap<git::RefString, Oid>);

impl Refs {
    /// Verify the given signature on these refs, and return [`SignedRefs`] on success.
    pub fn verified(
        self,
        signer: PublicKey,
        signature: Signature,
    ) -> Result<SignedRefs<Verified>, Error> {
        let refs = self;
        let msg = refs.canonical();

        match signer.verify(msg, &signature) {
            Ok(()) => Ok(SignedRefs {
                refs,
                signature,
                id: signer,
                _verified: PhantomData,
            }),
            Err(e) => Err(e.into()),
        }
    }

    /// Sign these refs with the given signer and return [`SignedRefs`].
    pub fn signed<G>(self, signer: &G) -> Result<SignedRefs<Verified>, Error>
    where
        G: Signer,
    {
        let refs = self;
        let msg = refs.canonical();
        let signature = signer.try_sign(&msg)?;

        Ok(SignedRefs {
            refs,
            signature,
            id: *signer.public_key(),
            _verified: PhantomData,
        })
    }

    /// Get a particular ref.
    pub fn get(&self, name: &git::Qualified) -> Option<Oid> {
        self.0.get(name.to_ref_string().as_refstr()).copied()
    }

    /// Get a particular head ref.
    pub fn head(&self, name: impl AsRef<git::RefStr>) -> Option<Oid> {
        let branch = git::refname!("refs/heads").join(name);
        self.0.get(&branch).copied()
    }

    /// Create refs from a canonical representation.
    pub fn from_canonical(bytes: &[u8]) -> Result<Self, canonical::Error> {
        let reader = BufReader::new(bytes);
        let mut refs = BTreeMap::new();

        for line in reader.lines() {
            let line = line?;
            let (oid, name) = line
                .split_once(' ')
                .ok_or(canonical::Error::InvalidFormat)?;

            let name = git::RefString::try_from(name)?;
            let oid = Oid::from_str(oid)?;

            if oid.is_zero() {
                continue;
            }
            refs.insert(name, oid);
        }
        Ok(Self(refs))
    }

    pub fn canonical(&self) -> Vec<u8> {
        let mut buf = String::new();

        for (name, oid) in self.iter() {
            buf.push_str(&oid.to_string());
            buf.push(' ');
            buf.push_str(name);
            buf.push('\n');
        }
        buf.into_bytes()
    }
}

impl IntoIterator for Refs {
    type Item = (git::RefString, Oid);
    type IntoIter = std::collections::btree_map::IntoIter<git::RefString, Oid>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<Refs> for BTreeMap<git::RefString, Oid> {
    fn from(refs: Refs) -> Self {
        refs.0
    }
}

impl<V> From<SignedRefs<V>> for Refs {
    fn from(signed: SignedRefs<V>) -> Self {
        signed.refs
    }
}

impl From<BTreeMap<git::RefString, Oid>> for Refs {
    fn from(refs: BTreeMap<git::RefString, Oid>) -> Self {
        Self(refs)
    }
}

impl Deref for Refs {
    type Target = BTreeMap<git::RefString, Oid>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Refs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Combination of [`Refs`] and a [`Signature`]. The signature is a cryptographic
/// signature over the refs. This allows us to easily verify if a set of refs
/// came from a particular key.
///
/// The type parameter keeps track of whether the signature was [`Verified`] or
/// [`Unverified`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignedRefs<V> {
    /// The signed refs.
    pub refs: Refs,
    /// The signature of the signer over the refs.
    #[serde(skip)]
    pub signature: Signature,
    /// This is the remote under which these refs exist, and the public key of the signer.
    pub id: PublicKey,

    #[serde(skip)]
    _verified: PhantomData<V>,
}

impl SignedRefs<Unverified> {
    pub fn new(refs: Refs, id: PublicKey, signature: Signature) -> Self {
        Self {
            refs,
            signature,
            id,
            _verified: PhantomData,
        }
    }

    pub fn verified(self) -> Result<SignedRefs<Verified>, crypto::Error> {
        match self.verify(&self.id) {
            Ok(()) => Ok(SignedRefs {
                refs: self.refs,
                signature: self.signature,
                id: self.id,
                _verified: PhantomData,
            }),
            Err(e) => Err(e),
        }
    }

    pub fn verify(&self, signer: &PublicKey) -> Result<(), crypto::Error> {
        let canonical = self.refs.canonical();

        match signer.verify(canonical, &self.signature) {
            Ok(()) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

impl SignedRefs<Verified> {
    pub fn load<S>(remote: RemoteId, repo: &S) -> Result<Self, Error>
    where
        S: ReadRepository,
    {
        let oid = repo.reference_oid(&remote, &SIGREFS_BRANCH)?;

        SignedRefs::load_at(oid, remote, repo)
    }

    pub fn load_at<S>(oid: Oid, remote: RemoteId, repo: &S) -> Result<Self, Error>
    where
        S: storage::ReadRepository,
    {
        let refs = repo.blob_at(oid, Path::new(REFS_BLOB_PATH))?;
        let signature = repo.blob_at(oid, Path::new(SIGNATURE_BLOB_PATH))?;
        let signature: crypto::Signature = signature.content().try_into()?;

        match remote.verify(refs.content(), &signature) {
            Ok(()) => {
                let refs = Refs::from_canonical(refs.content())?;

                Ok(Self {
                    refs,
                    signature,
                    id: remote,
                    _verified: PhantomData,
                })
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Save the signed refs to disk.
    /// This creates a new commit on the signed refs branch, and updates the branch pointer.
    pub fn save<S: WriteRepository>(&self, repo: &S) -> Result<Updated, Error> {
        let sigref = &SIGREFS_BRANCH;
        let remote = &self.id;
        let raw = repo.raw();

        // N.b. if the signatures match then there are no updates
        let parent = match SignedRefsAt::load(*remote, repo)? {
            Some(SignedRefsAt { sigrefs, at }) if sigrefs.signature == self.signature => {
                return Ok(Updated::Unchanged { oid: at })
            }
            Some(SignedRefsAt { at, .. }) => Some(raw.find_commit(*at)?),
            None => None,
        };

        let tree = {
            let refs_blob_oid = raw.blob(&self.canonical())?;
            let sig_blob_oid = raw.blob(self.signature.as_ref())?;

            let mut builder = raw.treebuilder(None)?;
            builder.insert(REFS_BLOB_PATH, refs_blob_oid, 0o100_644)?;
            builder.insert(SIGNATURE_BLOB_PATH, sig_blob_oid, 0o100_644)?;

            let oid = builder.write()?;

            raw.find_tree(oid)
        }?;

        let sigref = sigref.with_namespace(remote.into());
        let author = raw.signature()?;

        #[cfg(debug_assertions)]
        let author = if let Ok(s) = std::env::var("RAD_COMMIT_TIME") {
            // SAFETY: Only used in test code.
            #[allow(clippy::unwrap_used)]
            let timestamp = s.trim().parse::<i64>().unwrap();
            let time = git2::Time::new(timestamp, 0);
            git2::Signature::new("radicle", remote.to_string().as_str(), &time)?
        } else {
            author
        };

        let commit = raw.commit(
            Some(&sigref),
            &author,
            &author,
            "Update signed refs\n",
            &tree,
            &parent.iter().collect::<Vec<&git2::Commit>>(),
        );

        match commit {
            Ok(oid) => Ok(Updated::Updated { oid: oid.into() }),
            Err(e) => match (e.class(), e.code()) {
                (git2::ErrorClass::Object, git2::ErrorCode::Modified) => {
                    log::warn!("Concurrent modification of refs: {:?}", e);

                    Err(Error::Git(e))
                }
                _ => Err(e.into()),
            },
        }
    }

    pub fn unverified(self) -> SignedRefs<Unverified> {
        SignedRefs {
            refs: self.refs,
            signature: self.signature,
            id: self.id,
            _verified: PhantomData,
        }
    }
}

impl<V> Deref for SignedRefs<V> {
    type Target = Refs;

    fn deref(&self) -> &Self::Target {
        &self.refs
    }
}

/// The content-addressable information required to load a remote's
/// `rad/sigrefs`.
///
/// This can be used to [`RefsAt::load`] a [`SignedRefsAt`].
///
/// It can also be used for communicating announcements of updates
/// references to other nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefsAt {
    /// The remote namespace of the `rad/sigrefs`.
    pub remote: RemoteId,
    /// The commit SHA that `rad/sigrefs` points to.
    pub at: Oid,
}

impl RefsAt {
    pub fn new<S: ReadRepository>(repo: &S, remote: RemoteId) -> Result<Self, git::raw::Error> {
        let at = repo.reference_oid(&remote, &storage::refs::SIGREFS_BRANCH)?;
        Ok(RefsAt { remote, at })
    }

    pub fn load<S: ReadRepository>(&self, repo: &S) -> Result<SignedRefsAt, Error> {
        SignedRefsAt::load_at(self.at, self.remote, repo)
    }

    pub fn path(&self) -> &git::Qualified {
        &SIGREFS_BRANCH
    }
}

/// Verified [`SignedRefs`] that keeps track of their content address
/// [`Oid`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRefsAt {
    pub sigrefs: SignedRefs<Verified>,
    pub at: Oid,
}

impl SignedRefsAt {
    /// Load the [`SignedRefs`] found under `remote`'s [`SIGREFS_BRANCH`].
    ///
    /// This will return `None` if the branch was not found, all other
    /// errors are returned.
    pub fn load<S>(remote: RemoteId, repo: &S) -> Result<Option<Self>, Error>
    where
        S: ReadRepository,
    {
        let at = match repo.reference_oid(&remote, &SIGREFS_BRANCH) {
            Ok(at) => at,
            Err(e) if git::is_not_found_err(&e) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Self::load_at(at, remote, repo).map(Some)
    }

    pub fn load_at<S>(at: Oid, remote: RemoteId, repo: &S) -> Result<Self, Error>
    where
        S: storage::ReadRepository,
    {
        Ok(Self {
            sigrefs: SignedRefs::load_at(at, remote, repo)?,
            at,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&git::RefString, &Oid)> {
        self.sigrefs.refs.iter()
    }
}

impl Deref for SignedRefsAt {
    type Target = SignedRefs<Verified>;

    fn deref(&self) -> &Self::Target {
        &self.sigrefs
    }
}

pub mod canonical {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error(transparent)]
        InvalidRef(#[from] git::fmt::Error),
        #[error("invalid canonical format")]
        InvalidFormat,
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error(transparent)]
        Git(#[from] git2::Error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_canonical_roundtrip(refs: Refs) {
        let encoded = refs.canonical();
        let decoded = Refs::from_canonical(&encoded).unwrap();

        assert_eq!(refs, decoded);
    }
}
