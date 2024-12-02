#![warn(clippy::unwrap_used)]
pub mod cob;
pub mod transport;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crypto::Verified;
use once_cell::sync::Lazy;
use tempfile::TempDir;

use crate::git::canonical::Canonical;
use crate::identity::doc::DocError;
use crate::identity::{Doc, DocAt, RepoId};
use crate::identity::{Identity, Project};
use crate::node::device::Device;
use crate::node::SyncedAt;
use crate::storage::refs;
use crate::storage::refs::{Refs, SignedRefs, SignedRefsAt};
use crate::storage::{
    ReadRepository, ReadStorage, Remote, Remotes, RepositoryInfo, SetHead, SignRepository,
    WriteRepository, WriteStorage,
};
use crate::{git, node};

pub use crate::git::{
    ext, raw, refname, refspec, Oid, PatternStr, Qualified, RefError, RefString, UserInfo,
};
pub use crate::storage::{Error, RepositoryError};

use super::refs::RefsAt;
use super::{RemoteId, RemoteRepository, ValidateRepository};

pub static NAMESPACES_GLOB: Lazy<git::refspec::PatternString> =
    Lazy::new(|| git::refspec::pattern!("refs/namespaces/*"));
pub static SIGREFS_GLOB: Lazy<refspec::PatternString> =
    Lazy::new(|| git::refspec::pattern!("refs/namespaces/*/rad/sigrefs"));
pub static CANONICAL_IDENTITY: Lazy<git::Qualified> = Lazy::new(|| {
    git::Qualified::from_components(
        git::name::component!("rad"),
        git::name::component!("id"),
        None,
    )
});

/// A parsed Git reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub oid: git::Oid,
    pub name: RefString,
    pub namespace: Option<RemoteId>,
}

impl TryFrom<git2::Reference<'_>> for Ref {
    type Error = RefError;

    fn try_from(r: git2::Reference) -> Result<Self, Self::Error> {
        let name = r.name().ok_or(RefError::InvalidName)?;
        let (namespace, name) = match git::parse_ref_namespaced::<RemoteId>(name) {
            Ok((namespace, refname)) => (Some(namespace), refname.to_ref_string()),
            Err(RefError::MissingNamespace(refname)) => (None, refname),
            Err(err) => return Err(err),
        };
        let oid = r.resolve()?.target().ok_or(RefError::NoTarget)?;

        Ok(Self {
            namespace,
            name,
            oid: oid.into(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Storage {
    path: PathBuf,
    info: UserInfo,
}

impl ReadStorage for Storage {
    type Repository = Repository;

    fn info(&self) -> &UserInfo {
        &self.info
    }

    fn path(&self) -> &Path {
        self.path.as_path()
    }

    fn path_of(&self, rid: &RepoId) -> PathBuf {
        paths::repository(&self, rid)
    }

    fn contains(&self, rid: &RepoId) -> Result<bool, RepositoryError> {
        if paths::repository(&self, rid).exists() {
            let _ = self.repository(*rid)?.head()?;
            return Ok(true);
        }
        Ok(false)
    }

    fn repository(&self, rid: RepoId) -> Result<Self::Repository, RepositoryError> {
        Repository::open(paths::repository(self, &rid), rid)
    }

    fn repositories(&self) -> Result<Vec<RepositoryInfo>, Error> {
        let mut repos = Vec::new();

        for result in fs::read_dir(&self.path)? {
            let path = result?;

            // Skip non-directories.
            if !path.file_type()?.is_dir() {
                continue;
            }
            // Skip hidden files.
            if path.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            // Skip lock files.
            if let Some(ext) = path.path().extension() {
                if ext == "lock" {
                    continue;
                }
            }
            let rid = RepoId::try_from(path.file_name())
                .map_err(|_| Error::InvalidId(path.file_name()))?;

            let repo = match self.repository(rid) {
                Ok(repo) => repo,
                Err(e) => {
                    log::warn!(target: "storage", "Repository {rid} is invalid: {e}");
                    continue;
                }
            };
            let doc = match repo.identity_doc() {
                Ok(doc) => doc.into(),
                Err(e) => {
                    log::warn!(target: "storage", "Repository {rid} is invalid: looking up doc: {e}");
                    continue;
                }
            };

            // For performance reasons, we don't do a full repository check here.
            let head = match repo.head() {
                Ok((_, head)) => head,
                Err(e) => {
                    log::warn!(target: "storage", "Repository {rid} is invalid: looking up head: {e}");
                    continue;
                }
            };
            // Nb. This will be `None` if they were not found.
            let refs = refs::SignedRefsAt::load(self.info.key, &repo)?;
            let synced_at = refs
                .as_ref()
                .map(|r| node::SyncedAt::new(r.at, &repo))
                .transpose()?;

            repos.push(RepositoryInfo {
                rid,
                head,
                doc,
                refs,
                synced_at,
            });
        }
        Ok(repos)
    }
}

impl WriteStorage for Storage {
    type RepositoryMut = Repository;

    fn repository_mut(&self, rid: RepoId) -> Result<Self::RepositoryMut, RepositoryError> {
        Repository::open(paths::repository(self, &rid), rid)
    }

    fn create(&self, rid: RepoId) -> Result<Self::RepositoryMut, Error> {
        Repository::create(paths::repository(self, &rid), rid, &self.info)
    }

    fn clean(&self, rid: RepoId) -> Result<Vec<RemoteId>, RepositoryError> {
        let repo = self.repository(rid)?;
        // N.b. we remove the repository if the `local` peer has no
        // `rad/sigrefs`. There's no risk of them corrupting data.
        let has_sigrefs = SignedRefsAt::load(self.info.key, &repo)?.is_some();
        if has_sigrefs {
            repo.clean(&self.info.key)
        } else {
            let remotes = repo.remote_ids()?.collect::<Result<_, _>>()?;
            repo.remove()?;
            Ok(remotes)
        }
    }
}

impl Storage {
    /// Open a new storage instance and load its inventory.
    pub fn open<P: AsRef<Path>>(path: P, info: UserInfo) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();

        match fs::create_dir_all(&path) {
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(Error::Io(err)),
            Ok(()) => {}
        }
        Ok(Self { path, info })
    }

    /// Create a [`Repository`] in a temporary directory.
    ///
    /// N.b. it is important to keep the [`TempDir`] in scope while
    /// using the [`Repository`]. If it is dropped, any action on the
    /// `Repository` will fail.
    pub fn lock_repository(&self, rid: RepoId) -> Result<(Repository, TempDir), RepositoryError> {
        if self.contains(&rid)? {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("refusing to create '{}.lock'", rid),
            ))
            .into());
        }
        let tmp = tempfile::Builder::new()
            .prefix(&rid.canonical())
            .suffix(".lock")
            .tempdir_in(self.path())
            .map_err(Error::from)?;
        Ok((Repository::create(tmp.path(), rid, &self.info)?, tmp))
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn repositories_by_id<'a>(
        &self,
        mut rids: impl Iterator<Item = &'a RepoId>,
    ) -> Result<Vec<RepositoryInfo>, RepositoryError> {
        rids.try_fold(Vec::new(), |mut infos, rid| {
            let repo = self.repository(*rid)?;
            let (_, head) = repo.head()?;
            let refs = refs::SignedRefsAt::load(self.info.key, &repo)?;
            let synced_at = refs
                .as_ref()
                .map(|r| SyncedAt::new(r.at, &repo))
                .transpose()?;
            let info = RepositoryInfo {
                rid: *rid,
                head,
                doc: repo.identity_doc()?.into(),
                refs,
                synced_at,
            };
            infos.push(info);
            Ok(infos)
        })
    }

    pub fn inspect(&self) -> Result<(), RepositoryError> {
        for r in self.repositories()? {
            let rid = r.rid;
            let repo = self.repository(rid)?;

            for r in repo.raw().references()? {
                let r = r?;
                let name = r.name().ok_or(Error::InvalidRef)?;
                let oid = r.resolve()?.target().ok_or(Error::InvalidRef)?;

                println!("{} {oid} {name}", rid.urn());
            }
        }
        Ok(())
    }
}

/// Git implementation of [`WriteRepository`] using the `git2` crate.
pub struct Repository {
    /// The repository identifier (RID).
    pub id: RepoId,
    /// The backing Git repository.
    pub backend: git2::Repository,
}

/// A set of [`Validation`] errors that a caller **must use**.
#[must_use]
#[derive(Debug, Default)]
pub struct Validations(pub Vec<Validation>);

impl Validations {
    pub fn append(&mut self, vs: &mut Self) {
        self.0.append(&mut vs.0)
    }
}

impl IntoIterator for Validations {
    type Item = Validation;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Deref for Validations {
    type Target = Vec<Validation>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Validations {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Validation errors that can occur when verifying the layout of the
/// storage. These errors include checking the validity of the
/// `rad/sigrefs` contents and the identity of the repository.
#[derive(Debug, Error)]
pub enum Validation {
    #[error("found unsigned ref `{0}`")]
    UnsignedRef(RefString),
    #[error("{refname}: expected {expected}, but found {actual}")]
    MismatchedRef {
        expected: Oid,
        actual: Oid,
        refname: RefString,
    },
    #[error("missing `refs/namespaces/{remote}/{refname}`")]
    MissingRef {
        remote: RemoteId,
        refname: RefString,
    },
    #[error("missing `refs/namespaces/{0}/refs/rad/sigrefs`")]
    MissingRadSigRefs(RemoteId),
}

impl Repository {
    /// Open an existing repository.
    pub fn open<P: AsRef<Path>>(path: P, id: RepoId) -> Result<Self, RepositoryError> {
        let backend = git2::Repository::open_ext(
            path.as_ref(),
            git2::RepositoryOpenFlags::empty()
                | git2::RepositoryOpenFlags::BARE
                | git2::RepositoryOpenFlags::NO_DOTGIT
                | git2::RepositoryOpenFlags::NO_SEARCH,
            &[] as &[&std::ffi::OsStr],
        )?;

        Ok(Self { id, backend })
    }

    /// Create a new repository.
    pub fn create<P: AsRef<Path>>(path: P, id: RepoId, info: &UserInfo) -> Result<Self, Error> {
        let backend = git2::Repository::init_opts(
            &path,
            git2::RepositoryInitOptions::new()
                .bare(true)
                .no_reinit(true)
                .external_template(false),
        )?;
        let mut config = backend.config()?;

        config.set_str("user.name", &info.name())?;
        config.set_str("user.email", &info.email())?;

        Ok(Self { id, backend })
    }

    /// Remove an existing repository
    pub fn remove(&self) -> Result<(), Error> {
        let path = self.backend.path();
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }

    /// Remove all the remotes of a repository that are not the
    /// delegates of the repository or the local peer.
    ///
    /// N.b. failure to delete remotes or references will not result
    /// in an early exit. Instead, this method continues to delete the
    /// next available remote or reference.
    pub fn clean(&self, local: &RemoteId) -> Result<Vec<RemoteId>, RepositoryError> {
        let delegates = self
            .delegates()?
            .into_iter()
            .map(|did| *did)
            .collect::<BTreeSet<_>>();
        let mut deleted = Vec::new();
        for id in self.remote_ids()? {
            let id = match id {
                Ok(id) => id,
                Err(e) => {
                    log::error!(target: "storage", "Failed to clean up remote: {e}");
                    continue;
                }
            };

            // N.b. it is fatal to delete local or delegates
            if *local == id || delegates.contains(&id) {
                continue;
            }

            let glob = git::refname!("refs/namespaces")
                .join(git::Component::from(&id))
                .with_pattern(git::refspec::STAR);
            let refs = match self.references_glob(&glob) {
                Ok(refs) => refs,
                Err(e) => {
                    log::error!(target: "storage", "Failed to clean up remote '{id}': {e}");
                    continue;
                }
            };
            for (refname, _) in refs {
                if let Ok(mut r) = self.backend.find_reference(refname.as_str()) {
                    if let Err(e) = r.delete() {
                        log::error!(target: "storage", "Failed to clean up reference '{refname}': {e}");
                    }
                } else {
                    log::error!(target: "storage", "Failed to clean up reference '{refname}'");
                }
            }
            deleted.push(id);
        }

        Ok(deleted)
    }

    /// Create the repository's identity branch.
    pub fn init<G, S>(
        doc: &Doc,
        storage: &S,
        signer: &Device<G>,
    ) -> Result<(Self, git::Oid), RepositoryError>
    where
        G: crypto::signature::Signer<crypto::Signature>,
        S: WriteStorage,
    {
        let (doc_oid, doc_bytes) = doc.encode()?;
        let id = RepoId::from(doc_oid);
        let repo = Self::create(paths::repository(storage, &id), id, storage.info())?;
        let oid = repo.backend.blob(&doc_bytes)?; // Store document blob in repository.

        debug_assert_eq!(oid, *doc_oid);

        let commit = doc.init(&repo, signer)?;

        Ok((repo, commit))
    }

    pub fn inspect(&self) -> Result<(), Error> {
        for r in self.backend.references()? {
            let r = r?;
            let name = r.name().ok_or(Error::InvalidRef)?;
            let oid = r.resolve()?.target().ok_or(Error::InvalidRef)?;

            println!("{oid} {name}");
        }
        Ok(())
    }

    /// Iterate over all references.
    pub fn references(
        &self,
    ) -> Result<impl Iterator<Item = Result<Ref, refs::Error>> + '_, git2::Error> {
        let refs = self
            .backend
            .references()?
            .map(|reference| {
                let r = reference?;

                match Ref::try_from(r) {
                    Err(err) => Err(err.into()),
                    Ok(r) => Ok(Some(r)),
                }
            })
            .filter_map(Result::transpose);

        Ok(refs)
    }

    /// Get the canonical project information.
    pub fn project(&self) -> Result<Project, RepositoryError> {
        let head = self.identity_head()?;
        let doc = self.identity_doc_at(head)?;
        let proj = doc.project()?;

        Ok(proj)
    }

    pub fn identity_doc_of(&self, remote: &RemoteId) -> Result<Doc, DocError> {
        let oid = self.identity_head_of(remote)?;
        Doc::load_at(oid, self).map(|d| d.into())
    }

    pub fn remote_ids(
        &self,
    ) -> Result<impl Iterator<Item = Result<RemoteId, refs::Error>> + '_, git2::Error> {
        let iter = self.backend.references_glob(SIGREFS_GLOB.as_str())?.map(
            |reference| -> Result<RemoteId, refs::Error> {
                let r = reference?;
                let name = r.name().ok_or(refs::Error::InvalidRef)?;
                let (id, _) = git::parse_ref_namespaced::<RemoteId>(name)?;

                Ok(id)
            },
        );
        Ok(iter)
    }

    pub fn remotes(
        &self,
    ) -> Result<
        impl Iterator<Item = Result<(RemoteId, Remote<Verified>), refs::Error>> + '_,
        git2::Error,
    > {
        let remotes =
            self.backend
                .references_glob(SIGREFS_GLOB.as_str())?
                .map(|reference| -> Result<_, _> {
                    let r = reference?;
                    let name = r.name().ok_or(refs::Error::InvalidRef)?;
                    let (id, _) = git::parse_ref_namespaced::<RemoteId>(name)?;
                    let remote = self.remote(&id)?;

                    Ok((id, remote))
                });
        Ok(remotes)
    }
}

impl RemoteRepository for Repository {
    fn remotes(&self) -> Result<Remotes<Verified>, refs::Error> {
        let mut remotes = Vec::new();
        for remote in Repository::remotes(self)? {
            remotes.push(remote?);
        }
        Ok(Remotes::from_iter(remotes))
    }

    fn remote(&self, remote: &RemoteId) -> Result<Remote<Verified>, refs::Error> {
        let refs = SignedRefs::load(*remote, self)?;
        Ok(Remote::<Verified>::new(refs))
    }

    fn remote_refs_at(&self) -> Result<Vec<RefsAt>, refs::Error> {
        let mut all = Vec::new();

        for remote in self.remote_ids()? {
            let remote = remote?;
            let refs_at = RefsAt::new(self, remote)?;

            all.push(refs_at);
        }
        Ok(all)
    }
}

impl ValidateRepository for Repository {
    fn validate_remote(&self, remote: &Remote<Verified>) -> Result<Validations, Error> {
        // Contains a copy of the signed refs of this remote.
        let mut signed = BTreeMap::from((*remote.refs).clone());
        let mut failures = Validations::default();
        let mut has_sigrefs = false;

        // Check all repository references, making sure they are present in the signed refs map.
        for (refname, oid) in self.references_of(&remote.id)? {
            // Skip validation of the signed refs branch, as it is not part of `Remote`.
            if refname == refs::SIGREFS_BRANCH.to_ref_string() {
                has_sigrefs = true;
                continue;
            }
            if let Some(signed_oid) = signed.remove(&refname) {
                if oid != signed_oid {
                    failures.push(Validation::MismatchedRef {
                        refname,
                        expected: signed_oid,
                        actual: oid,
                    });
                }
            } else {
                failures.push(Validation::UnsignedRef(refname));
            }
        }

        if !has_sigrefs {
            failures.push(Validation::MissingRadSigRefs(remote.id));
        }

        // The refs that are left in the map, are ones that were signed, but are not
        // in the repository. If any are left, bail.
        if let Some((name, _)) = signed.into_iter().next() {
            failures.push(Validation::MissingRef {
                refname: name,
                remote: remote.id,
            });
        }

        // Nb. As it stands, it doesn't make sense to verify a single remote's identity branch,
        // since it is a COB.

        Ok(failures)
    }
}

impl ReadRepository for Repository {
    fn id(&self) -> RepoId {
        self.id
    }

    fn is_empty(&self) -> Result<bool, git2::Error> {
        Ok(self.remotes()?.next().is_none())
    }

    fn path(&self) -> &Path {
        self.backend.path()
    }

    fn blob_at<P: AsRef<Path>>(&self, commit: Oid, path: P) -> Result<git2::Blob, git::Error> {
        let commit = self.backend.find_commit(*commit)?;
        let tree = commit.tree()?;
        let entry = tree.get_path(path.as_ref())?;
        let obj = entry.to_object(&self.backend)?;
        let blob = obj.into_blob().map_err(|_| {
            git::Error::NotFound(git::NotFound::NoSuchBlob(
                path.as_ref().display().to_string(),
            ))
        })?;

        Ok(blob)
    }

    fn blob(&self, oid: Oid) -> Result<git2::Blob, git::Error> {
        self.backend.find_blob(oid.into()).map_err(git::Error::from)
    }

    fn reference(
        &self,
        remote: &RemoteId,
        name: &git::Qualified,
    ) -> Result<git2::Reference, git::Error> {
        let name = name.with_namespace(remote.into());
        self.backend.find_reference(&name).map_err(git::Error::from)
    }

    fn reference_oid(
        &self,
        remote: &RemoteId,
        reference: &git::Qualified,
    ) -> Result<Oid, git::raw::Error> {
        let name = reference.with_namespace(remote.into());
        let oid = self.backend.refname_to_id(&name)?;

        Ok(oid.into())
    }

    fn commit(&self, oid: Oid) -> Result<git2::Commit, git::Error> {
        self.backend
            .find_commit(oid.into())
            .map_err(git::Error::from)
    }

    fn revwalk(&self, head: Oid) -> Result<git2::Revwalk, git2::Error> {
        let mut revwalk = self.backend.revwalk()?;
        revwalk.push(head.into())?;

        Ok(revwalk)
    }

    fn contains(&self, oid: Oid) -> Result<bool, raw::Error> {
        self.backend.odb().map(|odb| odb.exists(oid.into()))
    }

    fn is_ancestor_of(&self, ancestor: Oid, head: Oid) -> Result<bool, git::Error> {
        self.backend
            .graph_descendant_of(head.into(), ancestor.into())
            .map_err(git::Error::from)
    }

    fn references_of(&self, remote: &RemoteId) -> Result<Refs, Error> {
        let entries = self
            .backend
            .references_glob(format!("refs/namespaces/{remote}/*").as_str())?;
        let mut refs = BTreeMap::new();

        for e in entries {
            let e = e?;
            let name = e.name().ok_or(Error::InvalidRef)?;
            let (_, refname) = git::parse_ref::<RemoteId>(name)?;
            let oid = e.resolve()?.target().ok_or(Error::InvalidRef)?;
            let (_, category, _, _) = refname.non_empty_components();

            if [
                git::name::HEADS,
                git::name::TAGS,
                git::name::NOTES,
                &git::name::component!("rad"),
                &git::name::component!("cobs"),
            ]
            .contains(&category.as_ref())
            {
                refs.insert(refname.into(), oid.into());
            }
        }
        Ok(refs.into())
    }

    fn references_glob(
        &self,
        pattern: &PatternStr,
    ) -> Result<Vec<(Qualified, Oid)>, git::ext::Error> {
        let mut refs = Vec::new();

        for r in self.backend.references_glob(pattern)? {
            let r = r?;
            let c = r.peel_to_commit()?;

            if let Some(name) = r
                .name()
                .and_then(|n| git::RefStr::try_from_str(n).ok())
                .and_then(git::Qualified::from_refstr)
            {
                refs.push((name.to_owned(), c.id().into()));
            }
        }
        Ok(refs)
    }

    fn identity_doc_at(&self, head: Oid) -> Result<DocAt, DocError> {
        Doc::load_at(head, self)
    }

    fn head(&self) -> Result<(Qualified, Oid), RepositoryError> {
        // If `HEAD` is already set locally, just return that.
        if let Ok(head) = self.backend.head() {
            if let Ok((name, oid)) = git::refs::qualified_from(&head) {
                return Ok((name.to_owned(), oid));
            }
        }
        self.canonical_head()
    }

    fn canonical_head(&self) -> Result<(Qualified, Oid), RepositoryError> {
        let doc = self.identity_doc()?;
        let project = doc.project()?;
        let branch_ref = git::refs::branch(project.default_branch());
        let raw = self.raw();
        let oid = Canonical::default_branch(self, &project, doc.delegates().into())?
            .quorum(doc.threshold(), raw)?;
        Ok((branch_ref, oid))
    }

    fn identity_head(&self) -> Result<Oid, RepositoryError> {
        let result = self
            .backend
            .refname_to_id(CANONICAL_IDENTITY.as_str())
            .map(Oid::from);

        match result {
            Ok(oid) => Ok(oid),
            Err(err) if git::ext::is_not_found_err(&err) => self.canonical_identity_head(),
            Err(err) => Err(err.into()),
        }
    }

    fn identity_head_of(&self, remote: &RemoteId) -> Result<Oid, git::ext::Error> {
        self.reference_oid(remote, &git::refs::storage::IDENTITY_BRANCH)
            .map_err(git::ext::Error::from)
    }

    fn identity_root(&self) -> Result<Oid, RepositoryError> {
        let oid = self.backend.refname_to_id(CANONICAL_IDENTITY.as_str())?;
        let root = self
            .revwalk(oid.into())?
            .last()
            .ok_or(RepositoryError::Doc(DocError::Missing))??;

        Ok(root.into())
    }

    fn identity_root_of(&self, remote: &RemoteId) -> Result<Oid, RepositoryError> {
        // Remotes that run newer clients will have this reference set. For older clients,
        // compute the root OID based on the identity head.
        if let Ok(root) = self.reference_oid(remote, &git::refs::storage::IDENTITY_ROOT) {
            return Ok(root);
        }
        let oid = self.identity_head_of(remote)?;
        let root = self
            .revwalk(oid)?
            .last()
            .ok_or(RepositoryError::Doc(DocError::Missing))??;

        Ok(root.into())
    }

    fn canonical_identity_head(&self) -> Result<Oid, RepositoryError> {
        for remote in self.remote_ids()? {
            let remote = remote?;
            // Nb. A remote may not have an identity document if the user has not contributed
            // any changes to the identity COB.
            let Ok(root) = self.identity_root_of(&remote) else {
                continue;
            };
            let blob = Doc::blob_at(root, self)?;

            // We've got an identity that goes back to the correct root.
            if blob.id() == **self.id {
                let identity = Identity::get(&root.into(), self)?;

                return Ok(identity.head());
            }
        }
        Err(DocError::Missing.into())
    }

    fn merge_base(&self, left: &Oid, right: &Oid) -> Result<Oid, git::ext::Error> {
        self.backend
            .merge_base(**left, **right)
            .map(Oid::from)
            .map_err(git::ext::Error::from)
    }
}

impl WriteRepository for Repository {
    fn set_head(&self) -> Result<SetHead, RepositoryError> {
        let head_ref = refname!("HEAD");
        let old = self
            .raw()
            .refname_to_id(&head_ref)
            .ok()
            .map(|oid| oid.into());

        let (branch_ref, new) = self.canonical_head()?;

        if old == Some(new) {
            return Ok(SetHead { old, new });
        }
        log::debug!(target: "storage", "Setting ref: {} -> {}", &branch_ref, new);
        self.raw()
            .reference(&branch_ref, *new, true, "set-local-branch (radicle)")?;

        log::debug!(target: "storage", "Setting ref: {} -> {}", head_ref, branch_ref);
        self.raw()
            .reference_symbolic(&head_ref, &branch_ref, true, "set-head (radicle)")?;

        Ok(SetHead { old, new })
    }

    fn set_identity_head_to(&self, commit: Oid) -> Result<(), RepositoryError> {
        log::debug!(target: "storage", "Setting ref: {} -> {}", *CANONICAL_IDENTITY, commit);
        self.raw().reference(
            CANONICAL_IDENTITY.as_str(),
            *commit,
            true,
            "set-local-branch (radicle)",
        )?;
        Ok(())
    }

    fn set_remote_identity_root_to(
        &self,
        remote: &RemoteId,
        root: Oid,
    ) -> Result<(), RepositoryError> {
        let refname = git::refs::storage::id_root(remote);

        self.raw()
            .reference(refname.as_str(), *root, true, "set-id-root (radicle)")?;

        Ok(())
    }

    fn set_user(&self, info: &UserInfo) -> Result<(), Error> {
        let mut config = self.backend.config()?;
        config.set_str("user.name", &info.name())?;
        config.set_str("user.email", &info.email())?;
        Ok(())
    }

    fn raw(&self) -> &git2::Repository {
        &self.backend
    }
}

impl SignRepository for Repository {
    fn sign_refs<G: crypto::signature::Signer<crypto::Signature>>(
        &self,
        signer: &Device<G>,
    ) -> Result<SignedRefs<Verified>, RepositoryError> {
        let remote = signer.public_key();
        // Ensure the root reference is set, which is checked during sigref verification.
        if self.identity_root_of(remote).is_err() {
            self.set_remote_identity_root(remote)?;
        }
        let mut refs = self.references_of(remote)?;
        // Don't sign the `rad/sigrefs` ref itself, and don't sign invalid OIDs.
        refs.retain(|name, oid| {
            name.as_refstr() != refs::SIGREFS_BRANCH.as_ref() && !oid.is_zero()
        });
        let signed = refs.signed(signer)?.verified(self)?;
        signed.save(self)?;

        Ok(signed)
    }
}

pub mod trailers {
    use std::str::FromStr;

    use thiserror::Error;

    use super::*;
    use crypto::{PublicKey, PublicKeyError};
    use crypto::{Signature, SignatureError};

    pub const SIGNATURE_TRAILER: &str = "Rad-Signature";

    #[derive(Error, Debug)]
    pub enum Error {
        #[error("invalid format for signature trailer")]
        SignatureTrailerFormat,
        #[error("invalid public key in signature trailer")]
        PublicKey(#[from] PublicKeyError),
        #[error("invalid signature in trailer")]
        Signature(#[from] SignatureError),
    }

    pub fn parse_signatures(msg: &str) -> Result<HashMap<PublicKey, Signature>, Error> {
        let trailers =
            git2::message_trailers_strs(msg).map_err(|_| Error::SignatureTrailerFormat)?;
        let mut signatures = HashMap::with_capacity(trailers.len());

        for (key, val) in trailers.iter() {
            if key == SIGNATURE_TRAILER {
                if let Some((pk, sig)) = val.split_once(' ') {
                    let pk = PublicKey::from_str(pk)?;
                    let sig = Signature::from_str(sig)?;

                    signatures.insert(pk, sig);
                } else {
                    return Err(Error::SignatureTrailerFormat);
                }
            }
        }
        Ok(signatures)
    }
}

pub mod paths {
    use std::path::PathBuf;

    use super::ReadStorage;
    use super::RepoId;

    pub fn repository<S: ReadStorage>(storage: &S, proj: &RepoId) -> PathBuf {
        storage.path().join(proj.canonical())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;
    use crate::git;
    use crate::storage::refs::SIGREFS_BRANCH;
    use crate::storage::{ReadRepository, ReadStorage};
    use crate::test::fixtures;

    #[test]
    fn test_remote_refs() {
        let dir = tempfile::tempdir().unwrap();
        let signer = Device::mock();
        let storage = fixtures::storage(dir.path(), &signer).unwrap();
        let inv = storage.repositories().unwrap();
        let proj = inv.first().unwrap();
        let mut refs = git::remote_refs(&git::Url::from(proj.rid)).unwrap();

        let project = storage.repository(proj.rid).unwrap();
        let remotes = project.remotes().unwrap();

        // Strip the remote refs of sigrefs so we can compare them.
        for remote in refs.values_mut() {
            let sigref = (*SIGREFS_BRANCH).to_ref_string();
            remote.remove(&sigref).unwrap();
        }

        let remotes = remotes
            .map(|remote| remote.map(|(id, r): (RemoteId, Remote<Verified>)| (id, r.refs.into())))
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(refs, remotes);
    }

    #[test]
    fn test_references_of() {
        let tmp = tempfile::tempdir().unwrap();
        let signer = Device::mock();
        let storage = Storage::open(tmp.path().join("storage"), fixtures::user()).unwrap();

        transport::local::register(storage.clone());

        let (rid, _, _, _) =
            fixtures::project(tmp.path().join("project"), &storage, &signer).unwrap();
        let repo = storage.repository(rid).unwrap();
        let id = repo.identity().unwrap().head();
        let cob = format!("refs/cobs/xyz.radicle.id/{id}");

        let mut refs = repo
            .references_of(signer.public_key())
            .unwrap()
            .iter()
            .map(|(r, _)| r.to_string())
            .collect::<Vec<_>>();
        refs.sort();

        assert_eq!(
            refs,
            vec![
                &cob,
                "refs/heads/master",
                "refs/rad/id",
                "refs/rad/root",
                "refs/rad/sigrefs"
            ]
        );
    }

    #[test]
    fn test_sign_refs() {
        let tmp = tempfile::tempdir().unwrap();
        let mut rng = fastrand::Rng::new();
        let signer = Device::mock_rng(&mut rng);
        let storage = Storage::open(tmp.path(), fixtures::user()).unwrap();
        let alice = *signer.public_key();
        let (rid, _, working, _) =
            fixtures::project(tmp.path().join("project"), &storage, &signer).unwrap();
        let stored = storage.repository(rid).unwrap();
        let sig = git2::Signature::now(&alice.to_string(), "anonymous@radicle.xyz").unwrap();
        let head = working.head().unwrap().peel_to_commit().unwrap();

        git::commit(
            &working,
            &head,
            &git::RefString::try_from(format!("refs/remotes/{alice}/heads/master")).unwrap(),
            "Second commit",
            &sig,
            &head.tree().unwrap(),
        )
        .unwrap();

        let signed = stored.sign_refs(&signer).unwrap();
        let remote = stored.remote(&alice).unwrap();
        let mut unsigned = stored.references_of(&alice).unwrap();

        // The signed refs doesn't contain the signature ref itself.
        let sigref = (*SIGREFS_BRANCH).to_ref_string();
        unsigned.remove(&sigref).unwrap();

        assert_eq!(remote.refs, signed);
        assert_eq!(*remote.refs, unsigned);
    }
}
