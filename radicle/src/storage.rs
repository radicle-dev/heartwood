pub mod git;
pub mod refs;

use std::collections::{hash_map, HashSet};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::{fmt, io};

use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crypto::{PublicKey, Unverified, Verified};
pub use git::{Validation, Validations};
pub use radicle_git_ext::Oid;

use crate::cob;
use crate::collections::RandomMap;
use crate::git::{canonical, ext as git_ext};
use crate::git::{refspec::Refspec, PatternString, Qualified, RefError, RefStr, RefString};
use crate::identity::{Did, PayloadError};
use crate::identity::{Doc, DocAt, DocError};
use crate::identity::{Identity, RepoId};
use crate::node::device::Device;
use crate::node::SyncedAt;
use crate::storage::git::NAMESPACES_GLOB;
use crate::storage::refs::Refs;

use self::git::UserInfo;
use self::refs::{RefsAt, SignedRefs};

pub type BranchName = git::RefString;

/// Basic repository information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryInfo {
    /// Repository identifier.
    pub rid: RepoId,
    /// Head of default branch.
    pub head: Oid,
    /// Identity document.
    pub doc: Doc,
    /// Local signed refs, if any.
    /// Repositories with this set to `None` are ones that are seeded but not forked.
    pub refs: Option<refs::SignedRefsAt>,
    /// Sync time of the repository.
    pub synced_at: Option<SyncedAt>,
}

/// Describes one or more namespaces.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum Namespaces {
    /// All namespaces.
    #[default]
    All,
    /// The followed set of namespaces.
    Followed(HashSet<PublicKey>),
}

impl Namespaces {
    pub fn to_refspecs(&self) -> Vec<Refspec<PatternString, PatternString>> {
        match self {
            Namespaces::All => vec![Refspec {
                src: (*NAMESPACES_GLOB).clone(),
                dst: (*NAMESPACES_GLOB).clone(),
                force: true,
            }],
            Namespaces::Followed(pks) => pks
                .iter()
                .map(|pk| {
                    let ns = pk.to_namespace().with_pattern(git::refspec::STAR);
                    Refspec {
                        src: ns.clone(),
                        dst: ns,
                        force: true,
                    }
                })
                .collect(),
        }
    }
}

impl FromIterator<PublicKey> for Namespaces {
    fn from_iter<T: IntoIterator<Item = PublicKey>>(iter: T) -> Self {
        Self::Followed(iter.into_iter().collect())
    }
}

/// Output of [`WriteRepository::set_head`].
pub struct SetHead {
    /// Old branch head.
    pub old: Option<Oid>,
    /// New branch head.
    pub new: Oid,
}

impl SetHead {
    /// Check if the head was updated.
    pub fn is_updated(&self) -> bool {
        self.old != Some(self.new)
    }
}

/// Repository error.
#[derive(Error, Debug)]
pub enum RepositoryError {
    #[error(transparent)]
    Storage(#[from] Error),
    #[error(transparent)]
    Store(#[from] cob::store::Error),
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error(transparent)]
    Payload(#[from] PayloadError),
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    GitExt(#[from] git_ext::Error),
    #[error(transparent)]
    Quorum(#[from] canonical::QuorumError),
    #[error(transparent)]
    Refs(#[from] refs::Error),
}

impl RepositoryError {
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::Storage(e) if e.is_not_found() => true,
            Self::Git(e) if git_ext::is_not_found_err(e) => true,
            Self::GitExt(git_ext::Error::NotFound(_)) => true,
            _ => false,
        }
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
    #[error("git: {0}")]
    Ext(#[from] git::ext::Error),
    #[error("invalid repository identifier {0:?}")]
    InvalidId(std::ffi::OsString),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
}

impl Error {
    /// Whether this error is caused by something not being found.
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::Io(e) if e.kind() == io::ErrorKind::NotFound => true,
            Self::Git(e) if git::ext::is_not_found_err(e) => true,
            Self::Doc(e) if e.is_not_found() => true,
            _ => false,
        }
    }
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
    #[error(transparent)]
    Storage(#[from] Error),
    #[error("failed to validate remote layouts in storage")]
    Validation { validations: Validations },
    #[error("repository head: {0}")]
    SetHead(#[from] DocError),
    #[error("repository: {0}")]
    Repository(#[from] RepositoryError),
}

pub type RemoteId = PublicKey;

/// An update to a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

    /// Get the old OID, if any.
    pub fn old(&self) -> Option<Oid> {
        match self {
            RefUpdate::Updated { old, .. } => Some(*old),
            RefUpdate::Created { .. } => None,
            RefUpdate::Deleted { oid, .. } => Some(*oid),
            RefUpdate::Skipped { oid, .. } => Some(*oid),
        }
    }

    /// Get the new OID, if any.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(&self) -> Option<Oid> {
        match self {
            RefUpdate::Updated { new, .. } => Some(*new),
            RefUpdate::Created { oid, .. } => Some(*oid),
            RefUpdate::Deleted { .. } => None,
            RefUpdate::Skipped { .. } => None,
        }
    }

    /// Get the ref name.
    pub fn name(&self) -> &RefStr {
        match self {
            RefUpdate::Updated { name, .. } => name.as_refstr(),
            RefUpdate::Created { name, .. } => name.as_refstr(),
            RefUpdate::Deleted { name, .. } => name.as_refstr(),
            RefUpdate::Skipped { name, .. } => name.as_refstr(),
        }
    }

    /// Is it an update.
    pub fn is_updated(&self) -> bool {
        matches!(self, RefUpdate::Updated { .. })
    }

    /// Is it a create.
    pub fn is_created(&self) -> bool {
        matches!(self, RefUpdate::Created { .. })
    }

    /// Is it a skip.
    pub fn is_skipped(&self) -> bool {
        matches!(self, RefUpdate::Skipped { .. })
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
pub struct Remotes<V>(RandomMap<RemoteId, Remote<V>>);

impl<V> FromIterator<(RemoteId, Remote<V>)> for Remotes<V> {
    fn from_iter<T: IntoIterator<Item = (RemoteId, Remote<V>)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<V> Deref for Remotes<V> {
    type Target = RandomMap<RemoteId, Remote<V>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> Remotes<V> {
    pub fn new(remotes: RandomMap<RemoteId, Remote<V>>) -> Self {
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
        Self(RandomMap::default())
    }
}

impl<V> IntoIterator for Remotes<V> {
    type Item = (RemoteId, Remote<V>);
    type IntoIter = hash_map::IntoIter<RemoteId, Remote<V>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<V> From<Remotes<V>> for RandomMap<RemoteId, Refs> {
    fn from(other: Remotes<V>) -> Self {
        let mut remotes = RandomMap::with_hasher(fastrand::Rng::new().into());

        for (k, v) in other.into_iter() {
            remotes.insert(k, v.refs.into());
        }
        remotes
    }
}

/// A project remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Remote<V = Verified> {
    /// Git references published under this remote, and their hashes.
    #[serde(flatten)]
    pub refs: SignedRefs<V>,
}

impl Remote<Unverified> {
    /// Create a new unverified remotes object.
    pub fn new(refs: impl Into<SignedRefs<Unverified>>) -> Self {
        Self { refs: refs.into() }
    }
}

impl Remote<Unverified> {
    pub fn verified<R: ReadRepository>(self, repo: &R) -> Result<Remote<Verified>, Error> {
        let refs = self.refs.verified(repo)?;

        Ok(Remote { refs })
    }
}

impl Remote<Verified> {
    /// Create a new unverified remotes object.
    pub fn new(refs: impl Into<SignedRefs<Verified>>) -> Self {
        Self { refs: refs.into() }
    }

    pub fn unverified(self) -> Remote<Unverified> {
        Remote {
            refs: self.refs.unverified(),
        }
    }

    pub fn to_refspecs(&self) -> Vec<Refspec<PatternString, PatternString>> {
        let ns = self.id.to_namespace();
        // Nb. the references in Refs are expected to be Qualified
        self.refs
            .iter()
            .map(|(name, _)| {
                let name = PatternString::from(ns.join(name));
                Refspec {
                    src: name.clone(),
                    dst: name,
                    force: true,
                }
            })
            .collect()
    }
}

impl<V> Deref for Remote<V> {
    type Target = SignedRefs<V>;

    fn deref(&self) -> &Self::Target {
        &self.refs
    }
}

/// Read-only operations on a storage instance.
pub trait ReadStorage {
    type Repository: ReadRepository;

    /// Get user info for this storage.
    fn info(&self) -> &UserInfo;
    /// Get the storage base path.
    fn path(&self) -> &Path;
    /// Get a repository's path.
    fn path_of(&self, rid: &RepoId) -> PathBuf;
    /// Check whether storage contains a repository.
    fn contains(&self, rid: &RepoId) -> Result<bool, RepositoryError>;
    /// Return all repositories (public and private).
    fn repositories(&self) -> Result<Vec<RepositoryInfo>, Error>;
    /// Open or create a read-only repository.
    fn repository(&self, rid: RepoId) -> Result<Self::Repository, RepositoryError>;
    /// Get a repository's identity if it exists.
    fn get(&self, rid: RepoId) -> Result<Option<Doc>, RepositoryError> {
        match self.repository(rid) {
            Ok(repo) => Ok(Some(repo.identity_doc()?.into())),
            Err(e) if e.is_not_found() => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Allows access to individual storage repositories.
pub trait WriteStorage: ReadStorage {
    type RepositoryMut: WriteRepository;

    /// Open a read-write repository.
    fn repository_mut(&self, rid: RepoId) -> Result<Self::RepositoryMut, RepositoryError>;
    /// Create a read-write repository.
    fn create(&self, rid: RepoId) -> Result<Self::RepositoryMut, Error>;

    /// Clean the repository found at `rid`.
    ///
    /// If the local peer has initialised `rad/sigrefs` by forking or
    /// creating any COBs, then this will delete all remote namespaces
    /// that are neither the local's or a delegate's.
    ///
    /// If the local peer has no initialised `rad/sigrefs`, then the
    /// repository will be entirely removed from storage.
    fn clean(&self, rid: RepoId) -> Result<Vec<RemoteId>, RepositoryError>;
}

/// Anything can return the [`RepoId`] that it is associated with.
pub trait HasRepoId {
    fn rid(&self) -> RepoId;
}

impl<T: ReadRepository> HasRepoId for T {
    fn rid(&self) -> RepoId {
        ReadRepository::id(self)
    }
}

/// Allows read-only access to a repository.
pub trait ReadRepository: Sized + ValidateRepository {
    /// Return the repository id.
    fn id(&self) -> RepoId;

    /// Returns `true` if there are no references in the repository.
    fn is_empty(&self) -> Result<bool, git2::Error>;

    /// The [`Path`] to the git repository.
    fn path(&self) -> &Path;

    /// Get a blob in this repository at the given commit and path.
    fn blob_at<P: AsRef<Path>>(&self, commit: Oid, path: P) -> Result<git2::Blob, git_ext::Error>;

    /// Get a blob in this repository, given its id.
    fn blob(&self, oid: Oid) -> Result<git2::Blob, git_ext::Error>;

    /// Get the head of this repository.
    ///
    /// Returns the reference pointed to by `HEAD` if it is set. Otherwise, computes the canonical
    /// head using [`ReadRepository::canonical_head`].
    ///
    /// Returns the [`Oid`] as well as the qualified reference name.
    fn head(&self) -> Result<(Qualified, Oid), RepositoryError>;

    /// Compute the canonical head of this repository.
    ///
    /// Ignores any existing `HEAD` reference.
    ///
    /// Returns the [`Oid`] as well as the qualified reference name.
    fn canonical_head(&self) -> Result<(Qualified, Oid), RepositoryError>;

    /// Get the head of the `rad/id` reference in this repository.
    ///
    /// Returns the reference pointed to by `rad/id` if it is set. Otherwise, computes the canonical
    /// `rad/id` using [`ReadRepository::canonical_identity_head`].
    fn identity_head(&self) -> Result<Oid, RepositoryError>;

    /// Get the identity head of a specific remote.
    fn identity_head_of(&self, remote: &RemoteId) -> Result<Oid, git::ext::Error>;

    /// Get the root commit of the canonical identity branch.
    fn identity_root(&self) -> Result<Oid, RepositoryError>;

    /// Get the root commit of the identity branch of a sepcific remote.
    fn identity_root_of(&self, remote: &RemoteId) -> Result<Oid, RepositoryError>;

    /// Load the identity history.
    fn identity(&self) -> Result<Identity, RepositoryError>
    where
        Self: cob::Store,
    {
        Identity::load(self)
    }

    /// Compute the canonical `rad/id` of this repository.
    ///
    /// Ignores any existing `rad/id` reference.
    fn canonical_identity_head(&self) -> Result<Oid, RepositoryError>;

    /// Compute the canonical identity document.
    fn canonical_identity_doc(&self) -> Result<DocAt, RepositoryError> {
        let head = self.canonical_identity_head()?;
        let doc = self.identity_doc_at(head)?;

        Ok(doc)
    }

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
    /// Returns `Err` if the commit did not exist.
    fn commit(&self, oid: Oid) -> Result<git2::Commit, git::ext::Error>;

    /// Perform a revision walk of a commit history starting from the given head.
    fn revwalk(&self, head: Oid) -> Result<git2::Revwalk, git2::Error>;

    /// Check if the underlying ODB contains the given `oid`.
    fn contains(&self, oid: Oid) -> Result<bool, git2::Error>;

    /// Check whether the given commit is an ancestor of another commit.
    fn is_ancestor_of(&self, ancestor: Oid, head: Oid) -> Result<bool, git::ext::Error>;

    /// Get the object id of a reference under the given remote.
    fn reference_oid(
        &self,
        remote: &RemoteId,
        reference: &Qualified,
    ) -> Result<Oid, git::raw::Error>;

    /// Get all references of the given remote.
    fn references_of(&self, remote: &RemoteId) -> Result<Refs, Error>;

    /// Get all references following a pattern.
    /// Skips references with names that are not parseable into [`Qualified`].
    ///
    /// This function always peels reference to the commit. For tags, this means the [`Oid`] of the
    /// commit pointed to by the tag is returned, and not the [`Oid`] of the tag itsself.
    fn references_glob(
        &self,
        pattern: &git::PatternStr,
    ) -> Result<Vec<(Qualified, Oid)>, git::ext::Error>;

    /// Get repository delegates.
    fn delegates(&self) -> Result<NonEmpty<Did>, RepositoryError> {
        let doc = self.identity_doc()?;

        Ok(doc.delegates().clone().into())
    }

    /// Get the repository's identity document.
    fn identity_doc(&self) -> Result<DocAt, RepositoryError> {
        let head = self.identity_head()?;
        let doc = self.identity_doc_at(head)?;

        Ok(doc)
    }

    /// Get the repository's identity document at a specific commit.
    fn identity_doc_at(&self, head: Oid) -> Result<DocAt, DocError>;

    /// Get the merge base of two commits.
    fn merge_base(&self, left: &Oid, right: &Oid) -> Result<Oid, git::ext::Error>;
}

/// Access the remotes of a repository.
pub trait RemoteRepository {
    /// Get the given remote.
    fn remote(&self, remote: &RemoteId) -> Result<Remote<Verified>, refs::Error>;

    /// Get all remotes.
    fn remotes(&self) -> Result<Remotes<Verified>, refs::Error>;

    /// Get [`RefsAt`] of all remotes.
    fn remote_refs_at(&self) -> Result<Vec<RefsAt>, refs::Error>;
}

pub trait ValidateRepository
where
    Self: RemoteRepository,
{
    /// Validate all remotes with [`ValidateRepository::validate_remote`].
    fn validate(&self) -> Result<Validations, Error> {
        let mut failures = Validations::default();
        for (_, remote) in self.remotes()? {
            failures.append(&mut self.validate_remote(&remote)?);
        }
        Ok(failures)
    }

    /// Validates a remote's signed refs and identity.
    ///
    /// Returns any ref found under that remote that isn't signed.
    /// If a signed ref is missing from the repository, an error is returned.
    fn validate_remote(&self, remote: &Remote<Verified>) -> Result<Validations, Error>;
}

/// Allows read-write access to a repository.
pub trait WriteRepository: ReadRepository + SignRepository {
    /// Set the repository head to the canonical branch.
    /// This computes the head based on the delegate set.
    fn set_head(&self) -> Result<SetHead, RepositoryError>;
    /// Set the repository 'rad/id' to the canonical commit, agreed by quorum.
    fn set_identity_head(&self) -> Result<Oid, RepositoryError> {
        let head = self.canonical_identity_head()?;
        self.set_identity_head_to(head)?;

        Ok(head)
    }
    /// Set the identity root reference to the canonical identity root commit.
    fn set_remote_identity_root(&self, remote: &RemoteId) -> Result<Oid, RepositoryError> {
        let root = self.identity_root()?;
        self.set_remote_identity_root_to(remote, root)?;

        Ok(root)
    }
    /// Set the identity root reference to the given commit.
    fn set_remote_identity_root_to(
        &self,
        remote: &RemoteId,
        root: Oid,
    ) -> Result<(), RepositoryError>;
    /// Set the repository 'rad/id' to the given commit.
    fn set_identity_head_to(&self, commit: Oid) -> Result<(), RepositoryError>;
    /// Set the user info of the Git repository.
    fn set_user(&self, info: &UserInfo) -> Result<(), Error>;
    /// Get the underlying git repository.
    fn raw(&self) -> &git2::Repository;
}

/// Allows signing refs.
pub trait SignRepository {
    /// Sign the repository's refs under the `refs/rad/sigrefs` branch.
    fn sign_refs<G>(&self, signer: &Device<G>) -> Result<SignedRefs<Verified>, RepositoryError>
    where
        G: crypto::signature::Signer<crypto::Signature>;
}

impl<T, S> ReadStorage for T
where
    T: Deref<Target = S>,
    S: ReadStorage + 'static,
{
    type Repository = S::Repository;

    fn info(&self) -> &UserInfo {
        self.deref().info()
    }

    fn path(&self) -> &Path {
        self.deref().path()
    }

    fn path_of(&self, rid: &RepoId) -> PathBuf {
        self.deref().path_of(rid)
    }

    fn contains(&self, rid: &RepoId) -> Result<bool, RepositoryError> {
        self.deref().contains(rid)
    }

    fn get(&self, rid: RepoId) -> Result<Option<Doc>, RepositoryError> {
        self.deref().get(rid)
    }

    fn repository(&self, rid: RepoId) -> Result<Self::Repository, RepositoryError> {
        self.deref().repository(rid)
    }

    fn repositories(&self) -> Result<Vec<RepositoryInfo>, Error> {
        self.deref().repositories()
    }
}

impl<T, S> WriteStorage for T
where
    T: Deref<Target = S>,
    S: WriteStorage + 'static,
{
    type RepositoryMut = S::RepositoryMut;

    fn repository_mut(&self, rid: RepoId) -> Result<Self::RepositoryMut, RepositoryError> {
        self.deref().repository_mut(rid)
    }

    fn create(&self, rid: RepoId) -> Result<Self::RepositoryMut, Error> {
        self.deref().create(rid)
    }

    fn clean(&self, rid: RepoId) -> Result<Vec<RemoteId>, RepositoryError> {
        self.deref().clean(rid)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_storage() {}
}
