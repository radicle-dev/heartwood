pub mod git;
pub mod refs;

use std::collections::hash_map;
use std::ops::Deref;
use std::path::Path;
use std::{fmt, io};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crypto::{PublicKey, Signer, Unverified, Verified};
pub use git::{ProjectError, VerifyError};
pub use radicle_git_ext::Oid;

use crate::collections::HashMap;
use crate::git::ext as git_ext;
use crate::git::{Qualified, RefError, RefString};
use crate::identity;
use crate::identity::doc::DocError;
use crate::identity::{Id, IdError};
use crate::storage::refs::Refs;

use self::refs::SignedRefs;

pub type BranchName = git::RefString;
pub type Inventory = Vec<Id>;

/// Describes one or more namespaces.
#[derive(Default, Debug, Clone)]
pub enum Namespaces {
    /// All namespaces.
    #[default]
    All,
    /// A single namespace, by public key.
    One(PublicKey),
}

impl Namespaces {
    pub fn as_fetchspec(&self) -> String {
        match self {
            Self::All => String::from("refs/namespaces/*:refs/namespaces/*"),
            Self::One(pk) => format!("refs/namespaces/{pk}/refs/*:refs/namespaces/{pk}/refs/*"),
        }
    }
}

impl From<PublicKey> for Namespaces {
    fn from(pk: PublicKey) -> Self {
        Self::One(pk)
    }
}

/// Storage error.
#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid git reference")]
    InvalidRef,
    #[error("identity doc: {0}")]
    Doc(#[from] DocError),
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
}

/// Fetch error.
#[derive(Error, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum FetchError {
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Refs(#[from] refs::Error),
    #[error("verify: {0}")]
    Verify(#[from] git::VerifyError),
    #[error(transparent)]
    Storage(#[from] Error),
    // TODO: This should wrap a more specific error.
    #[error("repository head: {0}")]
    SetHead(#[from] ProjectError),
}

pub type RemoteId = PublicKey;

/// An update to a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
                write!(f, "~ {old:.7}..{new:.7} {name}")
            }
            Self::Created { name, oid } => {
                write!(f, "* 0000000..{oid:.7} {name}")
            }
            Self::Deleted { name, oid } => {
                write!(f, "- {oid:.7}..0000000 {name}")
            }
            Self::Skipped { name, oid } => {
                write!(f, "= {oid:.7}..{oid:.7} {name}")
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Remote<V = Verified> {
    /// ID of remote.
    pub id: PublicKey,
    /// Git references published under this remote, and their hashes.
    #[serde(flatten)]
    pub refs: SignedRefs<V>,
    /// Whether this remote is a delegate for the project.
    pub delegate: bool,
}

impl<V> Remote<V> {
    // TODO(finto): This function seems out of place in the API for a couple of reasons:
    // * The SignedRefs aren't guaranteed to be by the `id`
    // * I could write `Remote::<Verified>::new(id, refs) and because of the above, it's a LIE
    pub fn new(id: PublicKey, refs: impl Into<SignedRefs<V>>) -> Self {
        Self {
            id,
            refs: refs.into(),
            delegate: false,
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
        })
    }
}

impl Remote<Verified> {
    pub fn unverified(self) -> Remote<Unverified> {
        Remote {
            id: self.id,
            refs: self.refs.unverified(),
            delegate: self.delegate,
        }
    }
}

/// Read-only operations on a storage instance.
pub trait ReadStorage {
    /// Get the storage base path.
    fn path(&self) -> &Path;
    /// Get an identity document of a repository under a given remote.
    fn get(
        &self,
        remote: &RemoteId,
        rid: Id,
    ) -> Result<Option<identity::Doc<Verified>>, ProjectError>;
    /// Check whether storage contains a repository.
    fn contains(&self, rid: &Id) -> Result<bool, ProjectError>;
    /// Get the inventory of repositories hosted under this storage.
    fn inventory(&self) -> Result<Inventory, Error>;
}

/// Allows access to individual storage repositories.
pub trait WriteStorage: ReadStorage {
    type Repository: WriteRepository;

    fn repository(&self, proj: Id) -> Result<Self::Repository, Error>;
}

/// Allows read-only access to a repository.
pub trait ReadRepository {
    /// Return the repository id.
    fn id(&self) -> Id;

    /// Returns `true` if there are no references in the repository.
    fn is_empty(&self) -> Result<bool, git2::Error>;

    /// The [`Path`] to the git repository.
    fn path(&self) -> &Path;

    /// Get a blob in this repository at the given commit and path.
    fn blob_at<'a>(&'a self, commit: Oid, path: &'a Path)
        -> Result<git2::Blob<'a>, git_ext::Error>;

    /// Verify all references in the repository, checking that they are signed
    /// as part of 'sigrefs'. Also verify that no signed reference is missing
    /// from the repository.
    fn verify(&self) -> Result<(), VerifyError>;

    /// Get the head of this repository.
    ///
    /// Returns the reference pointed to by `HEAD` if it is set. Otherwise, computes the canonical
    /// head using [`ReadRepository::canonical_head`].
    ///
    /// Returns the [`Oid`] as well as the qualified reference name.
    fn head(&self) -> Result<(Qualified, Oid), ProjectError>;

    /// Compute the canonical head of this repository.
    ///
    /// Ignores any existing `HEAD` reference.
    ///
    /// Returns the [`Oid`] as well as the qualified reference name.
    fn canonical_head(&self) -> Result<(Qualified, Oid), ProjectError>;

    /// Get the `reference` for the given `remote`.
    ///
    /// Returns `None` is the reference did not exist.
    fn reference(
        &self,
        remote: &RemoteId,
        reference: &Qualified,
    ) -> Result<git2::Reference, git_ext::Error>;

    /// Get the [`git2::Commit`] found using its `oid`.
    ///
    /// Returns `None` if the commit did not exist.
    fn commit(&self, oid: Oid) -> Result<git2::Commit, git_ext::Error>;

    /// Perform a revision walk of a commit history starting from the given head.
    fn revwalk(&self, head: Oid) -> Result<git2::Revwalk, git2::Error>;

    /// Get the object id of a reference under the given remote.
    fn reference_oid(
        &self,
        remote: &RemoteId,
        reference: &Qualified,
    ) -> Result<Oid, git_ext::Error>;

    /// Get all references of the given remote.
    fn references_of(&self, remote: &RemoteId) -> Result<Refs, Error>;

    /// Get the given remote.
    fn remote(&self, remote: &RemoteId) -> Result<Remote<Verified>, refs::Error>;

    /// Get all remotes.
    fn remotes(&self) -> Result<Remotes<Verified>, refs::Error>;

    /// Get the repository's identity document.
    fn identity_doc(&self) -> Result<(Oid, identity::Doc<Unverified>), ProjectError>;
}

/// Allows read-write access to a repository.
pub trait WriteRepository: ReadRepository {
    /// Set the repository head to the canonical branch.
    /// This computes the head based on the delegate set.
    fn set_head(&self) -> Result<Oid, ProjectError>;
    /// Sign the repository's refs under the `refs/rad/sigrefs` branch.
    fn sign_refs<G: Signer>(&self, signer: &G) -> Result<SignedRefs<Verified>, Error>;
    /// Get the underlying git repository.
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

    fn contains(&self, rid: &Id) -> Result<bool, ProjectError> {
        self.deref().contains(rid)
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
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_storage() {}
}
