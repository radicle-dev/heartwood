pub mod transport;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crypto::{Signer, Unverified, Verified};
use git_ref_format::refspec;
use once_cell::sync::Lazy;

use crate::git;
use crate::identity;
use crate::identity::project::{Identity, IdentityError};
use crate::identity::{Doc, Id};
use crate::storage::refs;
use crate::storage::refs::{Refs, SignedRefs};
use crate::storage::{
    Error, FetchError, Inventory, ReadRepository, ReadStorage, Remote, Remotes, WriteRepository,
    WriteStorage,
};

pub use crate::git::*;

use super::{Namespaces, RefUpdate, RemoteId};
use transport::remote;

pub static NAMESPACES_GLOB: Lazy<refspec::PatternString> =
    Lazy::new(|| refspec::pattern!("refs/namespaces/*"));
pub static SIGREFS_GLOB: Lazy<refspec::PatternString> =
    Lazy::new(|| refspec::pattern!("refs/namespaces/*/rad/sigrefs"));

// TODO: Is this is the wrong place for this type?
#[derive(Error, Debug)]
pub enum ProjectError {
    #[error("identity branches diverge from each other")]
    BranchesDiverge,
    #[error("identity branches are in an invalid state")]
    InvalidState,
    #[error("storage error: {0}")]
    Storage(#[from] Error),
    #[error("identity document error: {0}")]
    Doc(#[from] identity::project::DocError),
    #[error("identity verification error: {0}")]
    Verify(#[from] identity::project::VerificationError),
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
        match self.repository(proj)?.project_of(remote) {
            Ok(doc) => Ok(Some(doc)),

            Err(err) if err.is_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        self.projects()
    }
}

impl WriteStorage for Storage {
    type Repository = Repository;

    fn repository(&self, proj: Id) -> Result<Self::Repository, Error> {
        Repository::open(paths::repository(self, &proj), proj)
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

    pub fn projects(&self) -> Result<Vec<Id>, Error> {
        let mut projects = Vec::new();

        for result in fs::read_dir(&self.path)? {
            let path = result?;
            let id = Id::try_from(path.file_name())?;

            projects.push(id);
        }
        Ok(projects)
    }

    pub fn inspect(&self) -> Result<(), Error> {
        for proj in self.projects()? {
            let repo = self.repository(proj)?;

            for r in repo.raw().references()? {
                let r = r?;
                let name = r.name().ok_or(Error::InvalidRef)?;
                let oid = r.target().ok_or(Error::InvalidRef)?;

                println!("{} {} {}", proj, oid, name);
            }
        }
        Ok(())
    }
}

pub struct Repository {
    pub id: Id,
    pub(crate) backend: git2::Repository,
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("invalid remote `{0}`")]
    InvalidRemote(RemoteId),
    #[error("invalid target `{2}` for reference `{1}` of remote `{0}`")]
    InvalidRefTarget(RemoteId, RefString, git2::Oid),
    #[error("invalid reference")]
    InvalidRef,
    #[error("invalid identity: {0}")]
    InvalidIdentity(#[from] IdentityError),
    #[error("ref error: {0}")]
    Ref(#[from] git::RefError),
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

    /// Verify all references in the repository, checking that they are signed
    /// as part of 'sigrefs'. Also verify that no signed reference is missing
    /// from the repository.
    pub fn verify(&self) -> Result<(), VerifyError> {
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

    pub fn inspect(&self) -> Result<(), Error> {
        for r in self.backend.references()? {
            let r = r?;
            let name = r.name().ok_or(Error::InvalidRef)?;
            let oid = r.target().ok_or(Error::InvalidRef)?;

            println!("{} {}", oid, name);
        }
        Ok(())
    }

    pub fn identity(&self, remote: &RemoteId) -> Result<Identity<Oid>, IdentityError> {
        Identity::load(remote, self)
    }

    pub fn project_of(&self, remote: &RemoteId) -> Result<identity::Doc<Verified>, ProjectError> {
        let (doc, _) = identity::Doc::load(remote, self)?;
        let verified = doc.verified()?;

        Ok(verified)
    }

    /// Return the canonical identity [`git::Oid`] and document.
    pub fn project(&self) -> Result<(Oid, identity::Doc<Unverified>), ProjectError> {
        let mut heads = Vec::new();
        for remote in self.remote_ids()? {
            let remote = remote?;
            let oid = Doc::<Unverified>::head(&remote, self)?;

            heads.push(oid.into());
        }
        // Keep track of the longest identity branch.
        let mut longest = heads.pop().ok_or(ProjectError::InvalidState)?;

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

        Doc::load_at(longest.into(), self)
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

    fn references(&self, remote: &RemoteId) -> Result<Refs, Error> {
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

    fn project(&self) -> Result<Doc<Verified>, Error> {
        todo!()
    }

    fn project_identity(&self) -> Result<(Oid, identity::Doc<Unverified>), ProjectError> {
        Repository::project(self)
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
        let (_, project) = self.project_identity()?;
        let branch_ref = Qualified::from(lit::refs_heads(&project.default_branch));
        let raw = self.raw();

        let mut heads = Vec::new();
        for delegate in project.delegates.iter() {
            let r = self.reference_oid(&delegate.id, &branch_ref)?.into();

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
    /// Fetch all remotes of a project from the given URL.
    /// This is the primary way in which projects are updated on the network.
    ///
    /// Since we're operating in an untrusted network, we have to be take some precautions
    /// when fetching from a remote. We don't want to fetch straight into a public facing
    /// repository because if the updates were to be invalid, we'd be allowing others to
    /// read this invalid state. We also don't want to lock our repositories during the fetch
    /// or verification, as this will make the repositories unavailable. Therefore, we choose
    /// to perform the fetch into a "staging" copy of the given repository we're fetching, and
    /// then transfer the changes to the canonical, public copy of the repository.
    ///
    /// To do this, we first create a temporary directory, and clone the canonical repo into it.
    /// This local clone takes advantage of the fact that both repositories live on the same
    /// host (or even file-system). We now have a "staging" copy and the canonical copy.
    ///
    /// We then fetch the *remote* repo into the *staging* copy. We turn off pruning because we
    /// don't want to accidentally delete any objects before verification is complete.
    ///
    /// We proceed to verify the staging copy through the usual verification process.
    ///
    /// If verification succeeds, we fetch from the staging copy into the canonical repo,
    /// with pruning *on*, and discard the staging copy. If it fails, we just discard the
    /// staging copy.
    ///
    fn fetch(
        &mut self,
        node: &RemoteId,
        namespaces: impl Into<Namespaces>,
    ) -> Result<Vec<RefUpdate>, FetchError> {
        // The steps are summarized in the following diagram:
        //
        //     staging <- git-clone -- local (canonical) # create staging copy
        //     staging <- git-fetch -- remote            # fetch from remote
        //
        //     ... verify ...
        //
        //     local <- git-fetch -- staging             # fetch from staging copy
        //

        let namespace = match namespaces.into() {
            Namespaces::All => None,
            Namespaces::One(ns) => Some(ns),
        };

        let mut updates = Vec::new();
        let mut callbacks = git2::RemoteCallbacks::new();
        let tempdir = tempfile::tempdir()?;

        // Create staging copy.
        let staging = {
            let mut builder = git2::build::RepoBuilder::new();
            let path = tempdir.path().join("git");
            let staging_repo = builder
                .bare(true)
                // Using `clone_local` will try to hard-link the ODBs for better performance.
                // TODO: Due to this, I think we'll have to run GC when there is a failure.
                .clone_local(git2::build::CloneLocal::Local)
                .clone(
                    git::url::File::new(self.backend.path().to_path_buf())
                        .to_string()
                        .as_str(),
                    &path,
                )?;

            // In case we fetch an invalid update, we want to make sure nothing is deleted.
            let mut opts = git2::FetchOptions::default();
            opts.prune(git2::FetchPrune::Off);

            // Fetch from the remote into the staging copy.
            staging_repo
                .remote_anonymous(
                    remote::Url {
                        node: *node,
                        repo: self.id,
                        namespace,
                    }
                    .to_string()
                    .as_str(),
                )?
                .fetch(&["refs/*:refs/*"], Some(&mut opts), None)?;

            // Verify the staging copy as if it was the canonical copy.
            Repository {
                id: self.id,
                backend: staging_repo,
            }
            .verify()?;

            path
        };

        callbacks.update_tips(|name, old, new| {
            if let Ok(name) = git::RefString::try_from(name) {
                if name.to_namespaced().is_some() {
                    updates.push(RefUpdate::from(name, old, new));
                    // Returning `true` ensures the process is not aborted.
                    return true;
                }
            }
            log::warn!("Invalid ref `{}` detected; aborting fetch", name);

            false
        });

        {
            let mut remote = self
                .backend
                .remote_anonymous(git::url::File::new(staging).to_string().as_str())?;
            let mut opts = git2::FetchOptions::default();
            opts.remote_callbacks(callbacks);

            let refspec = if let Some(namespace) = namespace {
                format!("refs/namespaces/{namespace}/refs/*:refs/namespaces/{namespace}/refs/*")
            } else {
                "refs/namespaces/*:refs/namespaces/*".to_owned()
            };
            // TODO: Make sure we verify before pruning, as pruning may get us into
            // a state we can't roll back.
            opts.prune(git2::FetchPrune::On);
            // Fetch from the staging copy into the canonical repo.
            remote.fetch(&[refspec], Some(&mut opts), None)?;
        }
        // Set repository HEAD for git cloning support.
        self.set_head()?;

        Ok(updates)
    }

    fn set_head(&self) -> Result<Oid, ProjectError> {
        let head_ref = refname!("HEAD");
        let (branch_ref, head) = self.canonical_head()?;

        log::debug!("Setting ref {:?} -> {:?}", &branch_ref, head);
        self.raw()
            .reference(&branch_ref, *head, true, "set-local-branch (radicle)")?;

        log::debug!("Setting ref {:?} -> {:?}", head_ref, branch_ref);
        self.raw()
            .reference_symbolic(&head_ref, &branch_ref, true, "set-head (radicle)")?;

        Ok(head)
    }

    fn sign_refs<G: Signer>(&self, signer: &G) -> Result<SignedRefs<Verified>, Error> {
        let remote = signer.public_key();
        let refs = self.references(remote)?;
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

    pub fn parse_signatures(msg: &str) -> Result<Vec<(PublicKey, Signature)>, Error> {
        let trailers =
            git2::message_trailers_strs(msg).map_err(|_| Error::SignatureTrailerFormat)?;
        let mut signatures = Vec::with_capacity(trailers.len());

        for (key, val) in trailers.iter() {
            if key == SIGNATURE_TRAILER {
                if let Some((pk, sig)) = val.split_once(' ') {
                    let pk = PublicKey::from_str(pk)?;
                    let sig = Signature::from_str(sig)?;

                    signatures.push((pk, sig));
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
        storage.path().join(proj.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::{io, net, process, thread};

    use crypto::test::signer::MockSigner;

    use super::*;
    use crate::assert_matches;
    use crate::git;
    use crate::rad;
    use crate::storage::refs::SIGREFS_BRANCH;
    use crate::storage::{ReadRepository, ReadStorage, RefUpdate, WriteRepository};
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
    fn test_fetch() {
        let tmp = tempfile::tempdir().unwrap();
        let alice_signer = MockSigner::default();
        let alice_pk = *alice_signer.public_key();
        let alice = fixtures::storage(tmp.path().join("alice"), &alice_signer).unwrap();
        let bob = Storage::open(tmp.path().join("bob")).unwrap();
        let inventory = alice.inventory().unwrap();
        let proj = *inventory.first().unwrap();
        let repo = alice.repository(proj).unwrap();
        let remotes = repo.remotes().unwrap().collect::<Vec<_>>();
        let refname = Qualified::from_refstr(git::refname!("refs/heads/master")).unwrap();

        // Have Bob fetch Alice's refs.
        let updates = bob
            .repository(proj)
            .unwrap()
            .fetch(&alice_pk, alice_pk)
            .unwrap();

        // Three refs are created for each remote.
        assert_eq!(updates.len(), remotes.len() * 3);

        for update in updates {
            assert_matches!(
                update,
                RefUpdate::Created { name, .. } if name.starts_with("refs/namespaces")
            );
        }

        for remote in remotes {
            let (id, _) = remote.unwrap();
            let alice_repo = alice.repository(proj).unwrap();
            let alice_oid = alice_repo.reference(&id, &refname).unwrap();

            let bob_repo = bob.repository(proj).unwrap();
            let bob_oid = bob_repo.reference(&id, &refname).unwrap();

            assert_eq!(alice_oid.target(), bob_oid.target());
        }

        // Canonical HEAD is set correctly.
        let alice_repo = alice.repository(proj).unwrap();
        let bob_repo = bob.repository(proj).unwrap();

        assert_eq!(
            bob_repo.backend.head().unwrap().target().unwrap(),
            alice_repo.backend.head().unwrap().target().unwrap()
        );
    }

    #[test]
    fn test_fetch_update() {
        let tmp = tempfile::tempdir().unwrap();
        let alice = Storage::open(tmp.path().join("alice/storage")).unwrap();
        let bob = Storage::open(tmp.path().join("bob/storage")).unwrap();
        let alice_signer = MockSigner::new(&mut fastrand::Rng::new());
        let alice_id = alice_signer.public_key();
        let (proj_id, _, proj_repo, alice_head) =
            fixtures::project(tmp.path().join("alice/project"), &alice, &alice_signer).unwrap();
        let refname = Qualified::from_refstr(git::refname!("refs/heads/master")).unwrap();

        transport::remote::mock::register(alice_id, alice.path());

        // Have Bob fetch Alice's refs.
        let updates = bob
            .repository(proj_id)
            .unwrap()
            .fetch(alice_signer.public_key(), *alice_signer.public_key())
            .unwrap();
        // Three refs are created: the branch, the signature and the id.
        assert_eq!(updates.len(), 3);

        let alice_proj_storage = alice.repository(proj_id).unwrap();
        let alice_head = proj_repo.find_commit(alice_head).unwrap();
        let alice_head = git::commit(&proj_repo, &alice_head, &refname, "Making changes", "Alice")
            .unwrap()
            .id();
        git::push(&proj_repo, "rad", [(&refname, &refname)]).unwrap();
        alice_proj_storage.sign_refs(&alice_signer).unwrap();
        alice_proj_storage.set_head().unwrap();

        // Have Bob fetch Alice's new commit.
        let updates = bob
            .repository(proj_id)
            .unwrap()
            .fetch(alice_signer.public_key(), *alice_signer.public_key())
            .unwrap();
        // The branch and signature refs are updated.
        assert_matches!(
            updates.as_slice(),
            &[RefUpdate::Updated { .. }, RefUpdate::Updated { .. }]
        );

        // Bob's storage is updated.
        let bob_repo = bob.repository(proj_id).unwrap();
        let bob_master = bob_repo.reference(alice_id, &refname).unwrap();

        assert_eq!(bob_master.target().unwrap(), alice_head);
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
    #[ignore]
    // Test the remote transport using `git-upload-pack` and TCP streams.
    // Must be run on its own, since it tries to register the remote transport, which
    // will fail if the mock transport was already registered.
    fn test_upload_pack() {
        let tmp = tempfile::tempdir().unwrap();
        let signer = MockSigner::default();
        let remote = *signer.public_key();
        let storage = Storage::open(tmp.path().join("storage")).unwrap();
        let socket = net::TcpListener::bind(net::SocketAddr::from(([0, 0, 0, 0], 0))).unwrap();
        let addr = socket.local_addr().unwrap();
        let source_path = tmp.path().join("source");
        let target_path = tmp.path().join("target");
        let (source, _) = fixtures::repository(&source_path);

        transport::local::register(storage.clone());

        let (proj, _, _) = rad::init(
            &source,
            "radicle",
            "radicle",
            git::refname!("master"),
            &signer,
            &storage,
        )
        .unwrap();

        let t = thread::spawn(move || {
            let (stream, _) = socket.accept().unwrap();
            let repo = storage.repository(proj).unwrap();
            // NOTE: `GIT_PROTOCOL=version=2` doesn't work.
            let mut child = process::Command::new("git")
                .current_dir(repo.path())
                .arg("upload-pack")
                .arg("--strict") // The path to the git repo must be exact.
                .arg(".")
                .stdout(process::Stdio::piped())
                .stdin(process::Stdio::piped())
                .spawn()
                .unwrap();

            let mut stdin = child.stdin.take().unwrap();
            let mut stdout = child.stdout.take().unwrap();

            let mut stream_r = stream.try_clone().unwrap();
            let mut stream_w = stream;

            let t = thread::spawn(move || {
                let mut buf = [0u8; 1024];

                while let Ok(n) = stream_r.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if stdin.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            });
            io::copy(&mut stdout, &mut stream_w).unwrap();

            t.join().unwrap();
            child.wait().unwrap();
        });

        let mut updates = Vec::new();
        {
            let mut callbacks = git2::RemoteCallbacks::new();
            let mut opts = git2::FetchOptions::default();

            callbacks.update_tips(|name, _, _| {
                updates.push(name.to_owned());
                true
            });
            opts.remote_callbacks(callbacks);

            let target = git2::Repository::init_bare(target_path).unwrap();
            let stream = net::TcpStream::connect(addr).unwrap();

            // Register the `heartwood://` transport for this stream.
            transport::remote::register(remote, stream.try_clone().unwrap());

            // Fetch with the `heartwood://` transport.
            target
                .remote_anonymous(&format!("heartwood://{remote}/{proj}"))
                .unwrap()
                .fetch(
                    &["refs/namespaces/*:refs/namespaces/*"],
                    Some(&mut opts),
                    None,
                )
                .unwrap();

            stream.shutdown(net::Shutdown::Both).unwrap();

            t.join().unwrap();
        }

        assert_eq!(
            updates,
            vec![
                format!("refs/namespaces/{remote}/refs/heads/master"),
                format!("refs/namespaces/{remote}/refs/rad/id"),
                format!("refs/namespaces/{remote}/refs/rad/sigrefs")
            ]
        );
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

        git::commit(
            backend,
            &head,
            &git::RefString::try_from(format!("refs/remotes/{alice}/heads/master")).unwrap(),
            "Second commit",
            &alice.to_string(),
        )
        .unwrap();

        let signed = project.sign_refs(&signer).unwrap();
        let remote = project.remote(&alice).unwrap();
        let mut unsigned = project.references(&alice).unwrap();

        // The signed refs doesn't contain the signature ref itself.
        let sigref = (*SIGREFS_BRANCH).to_ref_string();
        unsigned.remove(&sigref).unwrap();

        assert_eq!(remote.refs, signed);
        assert_eq!(*remote.refs, unsigned);
    }
}
