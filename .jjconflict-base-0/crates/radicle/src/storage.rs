pub mod git;
pub mod refs;

use std::collections::{HashSet, hash_map};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::{fmt, io};

use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use crate::git::Oid;
use crypto::PublicKey;
pub use git::{Validation, Validations};

use crate::cob;
use crate::collections::RandomMap;
use crate::git::RefError;
use crate::git::canonical;
use crate::git::fmt::{Qualified, RefStr, RefString, refspec::PatternString, refspec::Refspec};
use crate::git::raw::ErrorExt as _;
use crate::identity::{Did, PayloadError, doc};
use crate::identity::{Doc, DocAt, DocError};
use crate::identity::{Identity, RepoId};
use crate::node::SyncedAt;
use crate::storage::git::NAMESPACES_GLOB;
use crate::storage::refs::{FeatureLevel, Refs, SignedRefs};

use self::refs::RefsAt;
use crate::git::UserInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignedRefsInfo {
    /// Repositories with this set to `None` are ones that are seeded but not forked.
    None,
    /// Local signed refs, if any.
    Some(refs::SignedRefs),
    NeedsMigration,
}

impl SignedRefsInfo {
    pub(crate) fn new(
        result: Result<Option<SignedRefs>, refs::sigrefs::read::error::Read>,
    ) -> Result<Self, refs::sigrefs::read::error::Read> {
        Ok(match result {
            Ok(Some(refs))
                if refs.feature_level() >= FeatureLevel::LATEST && refs.parent().is_some() =>
            {
                SignedRefsInfo::Some(refs)
            }
            Ok(Some(_)) => SignedRefsInfo::NeedsMigration,
            Ok(None) => SignedRefsInfo::None,
            Err(refs::sigrefs::read::error::Read::Downgrade { .. }) => {
                SignedRefsInfo::NeedsMigration
            }
            Err(err) => return Err(err),
        })
    }
}

/// Basic repository information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryInfo {
    /// Repository identifier.
    pub rid: RepoId,
    /// Head of default branch.
    pub head: Oid,
    /// Identity document.
    pub doc: Doc,
    /// Information about local signed references.
    pub refs: SignedRefsInfo,
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
                    let ns = pk
                        .to_namespace()
                        .with_pattern(crate::git::fmt::refspec::STAR);
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
    Storage(Box<Error>),
    #[error(transparent)]
    Store(#[from] cob::store::Error),
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error(transparent)]
    Payload(#[from] PayloadError),
    #[error(transparent)]
    Git(#[from] crate::git::raw::Error),
    #[error(transparent)]
    Quorum(#[from] canonical::error::QuorumError),
    #[error(transparent)]
    Refs(Box<refs::Error>),
    #[error("missing canonical reference rule for default branch")]
    MissingBranchRule,
    #[error("could not get the default branch rule: {0}")]
    DefaultBranchRule(#[from] doc::DefaultBranchRuleError),
    #[error("failed to get canonical reference rules: {0}")]
    CanonicalRefs(#[from] doc::CanonicalRefsError),
    #[error(transparent)]
    FindObjects(#[from] canonical::effects::FindObjectsError),
}

impl From<Error> for RepositoryError {
    fn from(err: Error) -> Self {
        Self::Storage(Box::new(err))
    }
}

impl From<refs::Error> for RepositoryError {
    fn from(err: refs::Error) -> Self {
        Self::Refs(Box::new(err))
    }
}

impl RepositoryError {
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::Storage(e) => e.is_not_found(),
            Self::Git(e) => e.is_not_found(),
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
    Git(#[from] crate::git::raw::Error),
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
            Self::Git(e) if e.is_not_found() => true,
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
    Git(#[from] crate::git::raw::Error),
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
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum RefUpdate {
    Updated {
        #[cfg_attr(
            feature = "schemars",
            schemars(with = "crate::schemars_ext::git::fmt::RefString")
        )]
        name: RefString,
        old: Oid,
        new: Oid,
    },
    Created {
        #[cfg_attr(
            feature = "schemars",
            schemars(with = "crate::schemars_ext::git::fmt::RefString")
        )]
        name: RefString,
        oid: Oid,
    },
    Deleted {
        #[cfg_attr(
            feature = "schemars",
            schemars(with = "crate::schemars_ext::git::fmt::RefString")
        )]
        name: RefString,
        oid: Oid,
    },
    Skipped {
        #[cfg_attr(feature = "schemars", schemars(with = "String"))]
        name: RefString,
        oid: Oid,
    },
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Remotes(RandomMap<RemoteId, Remote>);

impl FromIterator<(RemoteId, Remote)> for Remotes {
    fn from_iter<T: IntoIterator<Item = (RemoteId, Remote)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl Deref for Remotes {
    type Target = RandomMap<RemoteId, Remote>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Remotes {
    pub fn new(remotes: RandomMap<RemoteId, Remote>) -> Self {
        Self(remotes)
    }
}

impl IntoIterator for Remotes {
    type Item = (RemoteId, Remote);
    type IntoIter = hash_map::IntoIter<RemoteId, Remote>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<Remotes> for RandomMap<RemoteId, SignedRefs> {
    fn from(other: Remotes) -> Self {
        let mut remotes = RandomMap::with_hasher(fastrand::Rng::new().into());

        for (k, v) in other.into_iter() {
            remotes.insert(k, v.refs);
        }
        remotes
    }
}

/// A project remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Remote {
    /// Git references published under this remote, and their hashes.
    #[serde(flatten)]
    pub refs: SignedRefs,
}

impl Remote {
    /// Create a new remotes object.
    pub fn new(refs: impl Into<SignedRefs>) -> Self {
        Self { refs: refs.into() }
    }

    pub fn to_refspecs(&self) -> Vec<Refspec<PatternString, PatternString>> {
        let ns = self.id().to_namespace();
        // Nb. the references in Refs are expected to be Qualified
        self.refs
            .keys()
            .map(|name| {
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

impl Deref for Remote {
    type Target = SignedRefs;

    fn deref(&self) -> &Self::Target {
        &self.refs
    }
}

/// Read-only operations on a storage instance.
pub trait ReadStorage {
    type Repository: ReadRepository + self::refs::sigrefs::git::reference::Reader;

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
    fn is_empty(&self) -> Result<bool, crate::git::raw::Error>;

    /// The [`Path`] to the git repository.
    fn path(&self) -> &Path;

    /// Get a blob in this repository at the given commit and path.
    fn blob_at<P: AsRef<Path>>(
        &self,
        commit: Oid,
        path: P,
    ) -> Result<crate::git::raw::Blob<'_>, crate::git::raw::Error>;

    /// Get a blob in this repository, given its id.
    fn blob(&self, oid: Oid) -> Result<crate::git::raw::Blob<'_>, crate::git::raw::Error>;

    /// Get the head of this repository.
    ///
    /// Returns the reference pointed to by `HEAD` if it is set. Otherwise, computes the canonical
    /// head using [`ReadRepository::canonical_head`].
    ///
    /// Returns the [`Oid`] as well as the qualified reference name.
    fn head(&self) -> Result<(Qualified<'_>, Oid), RepositoryError>;

    /// Compute the canonical head of this repository.
    ///
    /// Ignores any existing `HEAD` reference.
    ///
    /// Returns the [`Oid`] as well as the qualified reference name.
    fn canonical_head(&self) -> Result<(Qualified<'_>, Oid), RepositoryError>;

    /// Get the head of the `rad/id` reference in this repository.
    ///
    /// Returns the reference pointed to by `rad/id` if it is set. Otherwise, computes the canonical
    /// `rad/id` using [`ReadRepository::canonical_identity_head`].
    fn identity_head(&self) -> Result<Oid, RepositoryError>;

    /// Get the identity head of a specific remote.
    fn identity_head_of(&self, remote: &RemoteId) -> Result<Oid, crate::git::raw::Error>;

    /// Get the root commit of the canonical identity branch.
    fn identity_root(&self) -> Result<Oid, RepositoryError>;

    /// Get the root commit of the identity branch of a specific remote.
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
    ) -> Result<crate::git::raw::Reference<'_>, crate::git::raw::Error>;

    /// Get the [`crate::git::raw::Commit`] found using its `oid`.
    ///
    /// Returns `Err` if the commit did not exist.
    fn commit(&self, oid: Oid) -> Result<crate::git::raw::Commit<'_>, crate::git::raw::Error>;

    /// Perform a revision walk of a commit history starting from the given head.
    fn revwalk(&self, head: Oid) -> Result<crate::git::raw::Revwalk<'_>, crate::git::raw::Error>;

    /// Check if the underlying ODB contains the given `oid`.
    fn contains(&self, oid: Oid) -> Result<bool, crate::git::raw::Error>;

    /// Check whether the given commit is an ancestor of another commit.
    fn is_ancestor_of(&self, ancestor: Oid, head: Oid) -> Result<bool, crate::git::raw::Error>;

    /// Get the object id of a reference under the given remote.
    fn reference_oid(
        &self,
        remote: &RemoteId,
        reference: &Qualified,
    ) -> Result<Oid, crate::git::raw::Error>;

    /// Get all references of the given remote.
    fn references_of(&self, remote: &RemoteId) -> Result<Refs, Error>;

    /// Get all references following a pattern.
    /// Skips references with names that are not parseable into [`Qualified`].
    ///
    /// This function always peels reference to the commit. For tags, this means the [`Oid`] of the
    /// commit pointed to by the tag is returned, and not the [`Oid`] of the tag itself.
    fn references_glob(
        &self,
        pattern: &crate::git::fmt::refspec::PatternStr,
    ) -> Result<Vec<(Qualified<'_>, Oid)>, crate::git::raw::Error>;

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
    fn merge_base(&self, left: &Oid, right: &Oid) -> Result<Oid, crate::git::raw::Error>;
}

/// Access the remotes of a repository.
pub trait RemoteRepository {
    /// Get the given remote.
    fn remote(&self, remote: &RemoteId) -> Result<Remote, refs::Error>;

    /// Get all remotes.
    fn remotes(&self) -> Result<Remotes, refs::Error>;

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
    fn validate_remote(&self, remote: &Remote) -> Result<Validations, Error>;
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
    fn raw(&self) -> &crate::git::raw::Repository;
}

/// Allows signing refs.
pub trait SignRepository {
    /// Sign the repository's refs under the `refs/rad/sigrefs` branch.
    fn sign_refs<Signer>(&self, signer: &Signer) -> Result<SignedRefs, RepositoryError>
    where
        Signer: crypto::signature::Keypair<VerifyingKey = crypto::PublicKey>,
        Signer: crypto::signature::Signer<crypto::Signature>,
        Signer: crypto::signature::Verifier<crypto::Signature>;

    /// Sign the repository's refs under the `refs/rad/sigrefs` branch, even if unchanged.
    ///
    /// Most users will prefer [`Self::sign_refs`].
    fn force_sign_refs<Signer>(&self, signer: &Signer) -> Result<SignedRefs, RepositoryError>
    where
        Signer: crypto::signature::Keypair<VerifyingKey = crypto::PublicKey>,
        Signer: crypto::signature::Signer<crypto::Signature>,
        Signer: crypto::signature::Verifier<crypto::Signature>;
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
