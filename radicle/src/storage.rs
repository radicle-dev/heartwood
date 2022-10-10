pub mod git;
pub mod refs;

use std::collections::hash_map;
use std::marker::PhantomData;
use std::ops::Deref;
use std::path::Path;
use std::{fmt, io};

use thiserror::Error;

pub use git::{ProjectError, VerifyError};
pub use radicle_git_ext::Oid;

use crate::collections::HashMap;
use crate::crypto;
use crate::crypto::{PublicKey, Signer, Unverified, Verified};
use crate::git::ext as git_ext;
use crate::git::Url;
use crate::git::{RefError, RefStr, RefString};
use crate::identity;
use crate::identity::{Id, IdError};
use crate::storage::refs::Refs;

use self::refs::SignedRefs;

pub type BranchName = git::RefString;
pub type Inventory = Vec<Id>;

/// Storage error.
#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid git reference")]
    InvalidRef,
    #[error("git reference error: {0}")]
    Ref(#[from] RefError),
    #[error(transparent)]
    Refs(#[from] refs::Error),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("id: {0}")]
    Id(#[from] IdError),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("invalid repository head")]
    InvalidHead,
}

/// Fetch error.
#[derive(Error, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum FetchError {
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("verify: {0}")]
    Verify(#[from] git::VerifyError),
    #[error(transparent)]
    Storage(#[from] Error),
}

pub type RemoteId = PublicKey;

/// An update to a reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefUpdate {
    Updated { name: RefString, old: Oid, new: Oid },
    Created { name: RefString, oid: Oid },
    Deleted { name: RefString, oid: Oid },
    Skipped { name: RefString, oid: Oid },
}

impl RefUpdate {
    pub fn from(name: RefString, old: impl Into<Oid>, new: impl Into<Oid>) -> Self {
        let old = old.into();
        let new = new.into();

        if old.is_zero() {
            Self::Created { name, oid: new }
        } else if new.is_zero() {
            Self::Deleted { name, oid: old }
        } else if old != new {
            Self::Updated { name, old, new }
        } else {
            Self::Skipped { name, oid: old }
        }
    }
}

impl fmt::Display for RefUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Updated { name, old, new } => {
                write!(f, "~ {:.7}..{:.7} {}", old, new, name)
            }
            Self::Created { name, oid } => {
                write!(f, "* 0000000..{:.7} {}", oid, name)
            }
            Self::Deleted { name, oid } => {
                write!(f, "- {:.7}..0000000 {}", oid, name)
            }
            Self::Skipped { name, oid } => {
                write!(f, "= {:.7}..{:.7} {}", oid, oid, name)
            }
        }
    }
}

/// Project remotes. Tracks the git state of a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remotes<V>(HashMap<RemoteId, Remote<V>>);

impl<V> FromIterator<(RemoteId, Remote<V>)> for Remotes<V> {
    fn from_iter<T: IntoIterator<Item = (RemoteId, Remote<V>)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<V> Deref for Remotes<V> {
    type Target = HashMap<RemoteId, Remote<V>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> Remotes<V> {
    pub fn new(remotes: HashMap<RemoteId, Remote<V>>) -> Self {
        Self(remotes)
    }
}

impl Remotes<Verified> {
    pub fn unverified(self) -> Remotes<Unverified> {
        Remotes(
            self.into_iter()
                .map(|(id, r)| (id, r.unverified()))
                .collect(),
        )
    }
}

impl<V> Default for Remotes<V> {
    fn default() -> Self {
        Self(HashMap::default())
    }
}

impl<V> IntoIterator for Remotes<V> {
    type Item = (RemoteId, Remote<V>);
    type IntoIter = hash_map::IntoIter<RemoteId, Remote<V>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<V> From<Remotes<V>> for HashMap<RemoteId, Refs> {
    fn from(other: Remotes<V>) -> Self {
        let mut remotes = HashMap::with_hasher(fastrand::Rng::new().into());

        for (k, v) in other.into_iter() {
            remotes.insert(k, v.refs.into());
        }
        remotes
    }
}

/// A project remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote<V> {
    /// ID of remote.
    pub id: PublicKey,
    /// Git references published under this remote, and their hashes.
    pub refs: SignedRefs<V>,
    /// Whether this remote is of a project delegate.
    pub delegate: bool,
    /// Whether the remote is verified or not, ie. whether its signed refs were checked.
    verified: PhantomData<V>,
}

impl<V> Remote<V> {
    pub fn new(id: PublicKey, refs: impl Into<SignedRefs<V>>) -> Self {
        Self {
            id,
            refs: refs.into(),
            delegate: false,
            verified: PhantomData,
        }
    }
}

impl Remote<Unverified> {
    pub fn verified(self) -> Result<Remote<Verified>, crypto::Error> {
        let refs = self.refs.verified(&self.id)?;

        Ok(Remote {
            id: self.id,
            refs,
            delegate: self.delegate,
            verified: PhantomData,
        })
    }
}

impl Remote<Verified> {
    pub fn unverified(self) -> Remote<Unverified> {
        Remote {
            id: self.id,
            refs: self.refs.unverified(),
            delegate: self.delegate,
            verified: PhantomData,
        }
    }
}

pub trait ReadStorage {
    fn path(&self) -> &Path;
    fn url(&self, proj: &Id) -> Url;
    fn get(
        &self,
        remote: &RemoteId,
        proj: Id,
    ) -> Result<Option<identity::Doc<Verified>>, ProjectError>;
    fn inventory(&self) -> Result<Inventory, Error>;
}

pub trait WriteStorage: ReadStorage {
    type Repository: WriteRepository;

    fn repository(&self, proj: Id) -> Result<Self::Repository, Error>;
    fn sign_refs<G: Signer>(
        &self,
        repository: &Self::Repository,
        signer: G,
    ) -> Result<SignedRefs<Verified>, Error>;
    fn fetch(&self, proj_id: Id, remote: &Url) -> Result<Vec<RefUpdate>, FetchError>;
}

pub trait ReadRepository {
    fn is_empty(&self) -> Result<bool, git2::Error>;
    fn path(&self) -> &Path;
    fn blob_at<'a>(&'a self, oid: Oid, path: &'a Path) -> Result<git2::Blob<'a>, git_ext::Error>;
    fn reference(
        &self,
        remote: &RemoteId,
        reference: &RefStr,
    ) -> Result<git2::Reference, git_ext::Error>;
    fn commit(&self, oid: Oid) -> Result<Option<git2::Commit>, git2::Error>;
    fn revwalk(&self, head: Oid) -> Result<git2::Revwalk, git2::Error>;
    fn reference_oid(&self, remote: &RemoteId, reference: &RefStr) -> Result<Oid, git_ext::Error>;
    fn references(&self, remote: &RemoteId) -> Result<Refs, Error>;
    fn remote(&self, remote: &RemoteId) -> Result<Remote<Verified>, refs::Error>;
    fn remotes(&self) -> Result<Remotes<Verified>, refs::Error>;
    /// Return the project associated with this repository.
    fn project(&self) -> Result<identity::Doc<Verified>, Error>;
    fn project_identity(&self) -> Result<(Oid, identity::Doc<Unverified>), ProjectError>;
}

pub trait WriteRepository: ReadRepository {
    fn fetch(&mut self, url: &Url) -> Result<Vec<RefUpdate>, FetchError>;
    fn raw(&self) -> &git2::Repository;
}

impl<T, S> ReadStorage for T
where
    T: Deref<Target = S>,
    S: ReadStorage + 'static,
{
    fn path(&self) -> &Path {
        self.deref().path()
    }

    fn url(&self, proj: &Id) -> Url {
        self.deref().url(proj)
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        self.deref().inventory()
    }

    fn get(
        &self,
        remote: &RemoteId,
        proj: Id,
    ) -> Result<Option<identity::Doc<Verified>>, ProjectError> {
        self.deref().get(remote, proj)
    }
}

impl<T, S> WriteStorage for T
where
    T: Deref<Target = S>,
    S: WriteStorage + 'static,
{
    type Repository = S::Repository;

    fn repository(&self, proj: Id) -> Result<Self::Repository, Error> {
        self.deref().repository(proj)
    }

    fn sign_refs<G: Signer>(
        &self,
        repository: &S::Repository,
        signer: G,
    ) -> Result<SignedRefs<Verified>, Error> {
        self.deref().sign_refs(repository, signer)
    }

    fn fetch(&self, proj_id: Id, remote: &Url) -> Result<Vec<RefUpdate>, FetchError> {
        self.deref().fetch(proj_id, remote)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_storage() {}
}
