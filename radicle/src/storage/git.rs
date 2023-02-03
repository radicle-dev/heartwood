pub mod cob;
pub mod transport;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crypto::{Signer, Unverified, Verified};
use git_ref_format::refspec;
use once_cell::sync::Lazy;

use crate::git;
use crate::identity;
use crate::identity::{doc, Doc, Id};
use crate::identity::{Identity, IdentityError, Project};
use crate::storage::refs;
use crate::storage::refs::{Refs, SignedRefs};
use crate::storage::{
    Error, Inventory, ReadRepository, ReadStorage, Remote, Remotes, WriteRepository, WriteStorage,
};

pub use crate::git::*;

use super::RemoteId;

pub static NAMESPACES_GLOB: Lazy<refspec::PatternString> =
    Lazy::new(|| refspec::pattern!("refs/namespaces/*"));
pub static SIGREFS_GLOB: Lazy<refspec::PatternString> =
    Lazy::new(|| refspec::pattern!("refs/namespaces/*/rad/sigrefs"));

/// A parsed Git reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub oid: git::Oid,
    pub name: RefString,
    pub namespace: Option<RemoteId>,
}

impl<'a> TryFrom<git2::Reference<'a>> for Ref {
    type Error = RefError;

    fn try_from(r: git2::Reference) -> Result<Self, Self::Error> {
        let name = r.name().ok_or(RefError::InvalidName)?;
        let (namespace, name) = match git::parse_ref_namespaced::<RemoteId>(name) {
            Ok((namespace, refname)) => (Some(namespace), refname.to_ref_string()),
            Err(RefError::MissingNamespace(refname)) => (None, refname),
            Err(err) => return Err(err),
        };
        let Some(oid) = r.target() else {
            // Ignore symbolic refs, eg. `HEAD`.
            return Err(RefError::Symbolic(name));
        };
        Ok(Self {
            namespace,
            name,
            oid: oid.into(),
        })
    }
}

// TODO: Is this is the wrong place for this type?
#[derive(Error, Debug)]
pub enum ProjectError {
    #[error("identity branches diverge from each other")]
    BranchesDiverge,
    #[error("identity branches missing")]
    MissingHeads,
    #[error("storage error: {0}")]
    Storage(#[from] Error),
    #[error("identity document error: {0}")]
    Doc(#[from] doc::DocError),
    #[error("payload error: {0}")]
    Payload(#[from] doc::PayloadError),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("git: {0}")]
    GitExt(#[from] git::Error),
    #[error("refs: {0}")]
    Refs(#[from] refs::Error),
}

impl ProjectError {
    /// Whether this error is caused by the project not being found.
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::Doc(doc) => doc.is_not_found(),
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Storage {
    path: PathBuf,
}

impl ReadStorage for Storage {
    fn path(&self) -> &Path {
        self.path.as_path()
    }

    fn get(&self, remote: &RemoteId, proj: Id) -> Result<Option<Doc<Verified>>, ProjectError> {
        // TODO: Don't create a repo here if it doesn't exist?
        // Perhaps for checking we could have a `contains` method?
        match self.repository(proj)?.identity_of(remote) {
            Ok(doc) => Ok(Some(doc)),

            Err(err) if err.is_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        self.repositories()
    }
}

impl WriteStorage for Storage {
    type Repository = Repository;

    fn repository(&self, rid: Id) -> Result<Self::Repository, Error> {
        Repository::open(paths::repository(self, &rid), rid)
    }
}

impl Storage {
    // TODO: Return a better error when not found.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, io::Error> {
        let path = path.as_ref().to_path_buf();

        match fs::create_dir_all(&path) {
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
            Ok(()) => {}
        }

        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn repositories(&self) -> Result<Vec<Id>, Error> {
        let mut repos = Vec::new();

        for result in fs::read_dir(&self.path)? {
            let path = result?;
            let rid = Id::try_from(path.file_name())?;
            let repo = self.repository(rid)?;

            // For performance reasons, we don't do a full repository check here.
            if let Err(e) = repo.head() {
                log::error!(target: "storage", "Repository {rid} is corrupted: looking up head: {e}");
                continue;
            }
            repos.push(rid);
        }
        Ok(repos)
    }

    pub fn inspect(&self) -> Result<(), Error> {
        for proj in self.repositories()? {
            let repo = self.repository(proj)?;

            for r in repo.raw().references()? {
                let r = r?;
                let name = r.name().ok_or(Error::InvalidRef)?;
                let oid = r.target().ok_or(Error::InvalidRef)?;

                println!("{} {oid} {name}", proj.urn());
            }
        }
        Ok(())
    }
}

pub struct Repository {
    pub id: Id,
    pub backend: git2::Repository,
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("invalid remote `{0}`")]
    InvalidRemote(RemoteId),
    #[error("invalid target `{2}` for reference `{1}` of remote `{0}`")]
    InvalidRefTarget(RemoteId, RefString, git2::Oid),
    #[error("invalid identity: {0}")]
    InvalidIdentity(#[from] IdentityError),
    #[error("refs error: {0}")]
    Refs(#[from] refs::Error),
    #[error("unknown reference `{1}` in remote `{0}`")]
    UnknownRef(RemoteId, git::RefString),
    #[error("missing reference `{1}` in remote `{0}`")]
    MissingRef(RemoteId, git::RefString),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
}

impl Repository {
    pub fn open<P: AsRef<Path>>(path: P, id: Id) -> Result<Self, Error> {
        let backend = match git2::Repository::open_bare(path.as_ref()) {
            Err(e) if ext::is_not_found_err(&e) => {
                let backend = git2::Repository::init_opts(
                    &path,
                    git2::RepositoryInitOptions::new()
                        .bare(true)
                        .no_reinit(true)
                        .external_template(false),
                )?;
                let mut config = backend.config()?;

                // TODO: Get ahold of user name and/or key.
                config.set_str("user.name", "radicle")?;
                config.set_str("user.email", "radicle@localhost")?;

                Ok(backend)
            }
            Ok(repo) => Ok(repo),
            Err(e) => Err(e),
        }?;

        Ok(Self { id, backend })
    }

    /// Create the repository's identity branch.
    pub fn init<G: Signer>(
        doc: &Doc<Verified>,
        remote: &RemoteId,
        storage: &Storage,
        signer: &G,
    ) -> Result<(Self, git::Oid), Error> {
        let (doc_oid, doc) = doc.encode()?;
        let id = Id::from(doc_oid);
        let repo = Self::open(paths::repository(storage, &id), id)?;
        let oid = Doc::init(
            doc.as_slice(),
            remote,
            &[(signer.public_key(), signer.sign(&doc))],
            repo.raw(),
        )?;

        Ok((repo, oid))
    }

    pub fn inspect(&self) -> Result<(), Error> {
        for r in self.backend.references()? {
            let r = r?;
            let name = r.name().ok_or(Error::InvalidRef)?;
            let oid = r.target().ok_or(Error::InvalidRef)?;

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
                    Err(RefError::Symbolic(_)) => Ok(None),
                    Err(err) => Err(err.into()),
                    Ok(r) => Ok(Some(r)),
                }
            })
            .filter_map(Result::transpose);

        Ok(refs)
    }

    pub fn identity(&self, remote: &RemoteId) -> Result<Identity<Oid>, IdentityError> {
        Identity::load(remote, self)
    }

    pub fn project_of(&self, remote: &RemoteId) -> Result<Project, ProjectError> {
        let doc = self.identity_of(remote)?;
        let proj = doc.project()?;

        Ok(proj)
    }

    pub fn identity_of(&self, remote: &RemoteId) -> Result<Doc<Verified>, ProjectError> {
        let (doc, _) = identity::Doc::load(remote, self)?;
        let verified = doc.verified()?;

        Ok(verified)
    }

    /// Return the canonical identity [`git::Oid`] and document.
    pub fn identity_doc(&self) -> Result<(Oid, identity::Doc<Unverified>), ProjectError> {
        let mut heads = Vec::new();
        for remote in self.remote_ids()? {
            let remote = remote?;
            let oid = Doc::<Unverified>::head(&remote, self)?;

            heads.push(oid.into());
        }
        // Keep track of the longest identity branch.
        let mut longest = heads.pop().ok_or(ProjectError::MissingHeads)?;

        for head in &heads {
            let base = self.raw().merge_base(*head, longest)?;

            if base == longest {
                // `head` is a successor of `longest`. Update `longest`.
                //
                //   o head
                //   |
                //   o longest (base)
                //   |
                //
                longest = *head;
            } else if base == *head || *head == longest {
                // `head` is an ancestor of `longest`, or equal to it. Do nothing.
                //
                //   o longest             o longest, head (base)
                //   |                     |
                //   o head (base)   OR    o
                //   |                     |
                //
            } else {
                // The merge base between `head` and `longest` (`base`)
                // is neither `head` nor `longest`. Therefore, the branches have
                // diverged.
                //
                //    longest   head
                //           \ /
                //            o (base)
                //            |
                //
                return Err(ProjectError::BranchesDiverge);
            }
        }

        Doc::<Unverified>::load_at(longest.into(), self)
            .map(|(doc, _)| (longest.into(), doc))
            .map_err(ProjectError::from)
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

    /// Return all references that are namespaced, ie. that are signed by a node and verified.
    fn namespaced_references(
        &self,
    ) -> Result<impl Iterator<Item = Result<(RemoteId, Qualified, Oid), refs::Error>>, git2::Error>
    {
        let refs = self.backend.references_glob("refs/namespaces/*")?;
        let refs = refs
            .map(|reference| {
                let r = reference?;
                let name = r.name().ok_or(refs::Error::InvalidRef)?;
                let (namespace, refname) = git::parse_ref_namespaced::<RemoteId>(name)?;
                let Some(oid) = r.target() else {
                    // Ignore symbolic refs, eg. `HEAD`.
                    return Ok(None);
                };

                if refname == *refs::SIGREFS_BRANCH {
                    // Ignore the signed-refs reference, as this is what we're verifying.
                    return Ok(None);
                }
                Ok(Some((namespace, refname.to_owned(), oid.into())))
            })
            .filter_map(Result::transpose);

        Ok(refs)
    }
}

impl ReadRepository for Repository {
    fn id(&self) -> Id {
        self.id
    }

    fn is_empty(&self) -> Result<bool, git2::Error> {
        Ok(self.remotes()?.next().is_none())
    }

    fn path(&self) -> &Path {
        self.backend.path()
    }

    fn blob_at<'a>(&'a self, oid: Oid, path: &'a Path) -> Result<git2::Blob<'a>, git::Error> {
        git::ext::Blob::At {
            object: oid.into(),
            path,
        }
        .get(&self.backend)
    }

    fn verify(&self) -> Result<(), VerifyError> {
        let mut remotes: HashMap<RemoteId, Refs> = self
            .remotes()?
            .map(|remote| {
                let (id, remote) = remote?;
                Ok((id, remote.refs.into()))
            })
            .collect::<Result<_, VerifyError>>()?;

        for entry in self.namespaced_references()? {
            let (remote_id, refname, oid) = entry?;
            let remote = remotes
                .get_mut(&remote_id)
                .ok_or(VerifyError::InvalidRemote(remote_id))?;
            let refname = RefString::from(refname);
            let signed_oid = remote
                .remove(&refname)
                .ok_or_else(|| VerifyError::UnknownRef(remote_id, refname.clone()))?;

            if oid != signed_oid {
                return Err(VerifyError::InvalidRefTarget(remote_id, refname, *oid));
            }
        }

        for (remote, refs) in remotes.into_iter() {
            // The refs that are left in the map, are ones that were signed, but are not
            // in the repository.
            if let Some((name, _)) = refs.into_iter().next() {
                return Err(VerifyError::MissingRef(remote, name));
            }
            // Verify identity history of remote.
            self.identity(&remote)?.verified(self.id)?;
        }

        Ok(())
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
    ) -> Result<Oid, git::Error> {
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

    fn remote(&self, remote: &RemoteId) -> Result<Remote<Verified>, refs::Error> {
        let refs = SignedRefs::load(remote, self)?;
        Ok(Remote::new(*remote, refs))
    }

    fn references_of(&self, remote: &RemoteId) -> Result<Refs, Error> {
        // TODO: Only return known refs, eg. heads/ rad/ tags/ etc..
        let entries = self
            .backend
            .references_glob(format!("refs/namespaces/{remote}/*").as_str())?;
        let mut refs = BTreeMap::new();

        for e in entries {
            let e = e?;
            let name = e.name().ok_or(Error::InvalidRef)?;
            let (_, refname) = git::parse_ref::<RemoteId>(name)?;
            let oid = e.target().ok_or(Error::InvalidRef)?;

            refs.insert(refname.into(), oid.into());
        }
        Ok(refs.into())
    }

    fn remotes(&self) -> Result<Remotes<Verified>, refs::Error> {
        let mut remotes = Vec::new();
        for remote in Repository::remotes(self)? {
            remotes.push(remote?);
        }
        Ok(Remotes::from_iter(remotes))
    }

    fn identity_doc(&self) -> Result<(Oid, identity::Doc<Unverified>), ProjectError> {
        Repository::identity_doc(self)
    }

    fn head(&self) -> Result<(Qualified, Oid), ProjectError> {
        // If `HEAD` is already set locally, just return that.
        if let Ok(head) = self.backend.head() {
            if let Ok((name, oid)) = git::refs::qualified_from(&head) {
                return Ok((name.to_owned(), oid));
            }
        }
        self.canonical_head()
    }

    fn canonical_head(&self) -> Result<(Qualified, Oid), ProjectError> {
        // TODO: In the `fork` function for example, we call Repository::project_identity again,
        // This should only be necessary once.
        let (_, doc) = self.identity_doc()?;
        let doc = doc.verified()?;
        let project = doc.project()?;
        let branch_ref = Qualified::from(lit::refs_heads(&project.default_branch()));
        let raw = self.raw();

        let mut heads = Vec::new();
        for delegate in doc.delegates.iter() {
            let r = self.reference_oid(delegate, &branch_ref)?.into();

            heads.push(r);
        }

        let oid = match heads.as_slice() {
            [head] => Ok(*head),
            // FIXME: This branch is not tested.
            heads => raw.merge_base_many(heads),
        }?;

        Ok((branch_ref, oid.into()))
    }
}

impl WriteRepository for Repository {
    fn set_head(&self) -> Result<Oid, ProjectError> {
        let head_ref = refname!("HEAD");
        let (branch_ref, head) = self.canonical_head()?;

        log::debug!(target: "storage", "Setting ref: {} -> {}", &branch_ref, head);
        self.raw()
            .reference(&branch_ref, *head, true, "set-local-branch (radicle)")?;

        log::debug!(target: "storage", "Setting ref: {} -> {}", head_ref, branch_ref);
        self.raw()
            .reference_symbolic(&head_ref, &branch_ref, true, "set-head (radicle)")?;

        Ok(head)
    }

    fn sign_refs<G: Signer>(&self, signer: &G) -> Result<SignedRefs<Verified>, Error> {
        let remote = signer.public_key();
        let refs = self.references_of(remote)?;
        let signed = refs.signed(signer)?;

        signed.save(remote, self)?;

        Ok(signed)
    }

    fn raw(&self) -> &git2::Repository {
        &self.backend
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

    use super::Id;
    use super::ReadStorage;

    pub fn repository<S: ReadStorage>(storage: &S, proj: &Id) -> PathBuf {
        storage.path().join(proj.canonical())
    }
}

#[cfg(test)]
mod tests {
    use crypto::test::signer::MockSigner;

    use super::*;
    use crate::git;
    use crate::storage::refs::SIGREFS_BRANCH;
    use crate::storage::{ReadRepository, ReadStorage, WriteRepository};
    use crate::test::arbitrary;
    use crate::test::fixtures;

    #[test]
    fn test_remote_refs() {
        let dir = tempfile::tempdir().unwrap();
        let signer = MockSigner::default();
        let storage = fixtures::storage(dir.path(), &signer).unwrap();
        let inv = storage.inventory().unwrap();
        let proj = inv.first().unwrap();
        let mut refs = git::remote_refs(&git::Url::from(*proj)).unwrap();

        let project = storage.repository(*proj).unwrap();
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
    fn test_namespaced_references() {
        let tmp = tempfile::tempdir().unwrap();
        let signer = MockSigner::default();
        let storage = Storage::open(tmp.path().join("storage")).unwrap();

        transport::local::register(storage.clone());

        let (id, _, _, _) =
            fixtures::project(tmp.path().join("project"), &storage, &signer).unwrap();
        let proj = storage.repository(id).unwrap();

        let mut refs = proj
            .namespaced_references()
            .unwrap()
            .map(|r| r.unwrap())
            .map(|(_, r, _)| r.to_string())
            .collect::<Vec<_>>();
        refs.sort();

        assert_eq!(refs, vec!["refs/heads/master", "refs/rad/id"]);
    }

    #[test]
    fn test_sign_refs() {
        let tmp = tempfile::tempdir().unwrap();
        let mut rng = fastrand::Rng::new();
        let signer = MockSigner::new(&mut rng);
        let storage = Storage::open(tmp.path()).unwrap();
        let proj_id = arbitrary::gen::<Id>(1);
        let alice = *signer.public_key();
        let project = storage.repository(proj_id).unwrap();
        let backend = &project.backend;
        let sig = git2::Signature::now(&alice.to_string(), "anonymous@radicle.xyz").unwrap();
        let head = git::initial_commit(backend, &sig).unwrap();
        let tree =
            git::write_tree(Path::new("README"), "Hello World!\n".as_bytes(), backend).unwrap();

        git::commit(
            backend,
            &head,
            &git::RefString::try_from(format!("refs/remotes/{alice}/heads/master")).unwrap(),
            "Second commit",
            &sig,
            &tree,
        )
        .unwrap();

        let signed = project.sign_refs(&signer).unwrap();
        let remote = project.remote(&alice).unwrap();
        let mut unsigned = project.references_of(&alice).unwrap();

        // The signed refs doesn't contain the signature ref itself.
        let sigref = (*SIGREFS_BRANCH).to_ref_string();
        unsigned.remove(&sigref).unwrap();

        assert_eq!(remote.refs, signed);
        assert_eq!(*remote.refs, unsigned);
    }
}
