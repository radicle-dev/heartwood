use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fmt, fs, io};

use git_ref_format::refspec;
use once_cell::sync::Lazy;

pub use radicle_git_ext::Oid;

use crate::crypto::{Signer, Verified};
use crate::git;
use crate::identity::{self, IDENTITY_PATH};
use crate::identity::{Id, Project, PublicKey};
use crate::storage::refs;
use crate::storage::refs::{Refs, SignedRefs};
use crate::storage::{
    Error, Inventory, ReadRepository, ReadStorage, Remote, WriteRepository, WriteStorage,
};

use super::{RefUpdate, RemoteId};

pub static RADICLE_ID_REF: Lazy<git::RefString> = Lazy::new(|| git::refname!("heads/radicle/id"));
pub static REMOTES_GLOB: Lazy<refspec::PatternString> =
    Lazy::new(|| refspec::pattern!("refs/remotes/*"));
pub static SIGNATURES_GLOB: Lazy<refspec::PatternString> =
    Lazy::new(|| refspec::pattern!("refs/remotes/*/radicle/signature"));

pub struct Storage {
    path: PathBuf,
    signer: Arc<dyn Signer>,
}

impl fmt::Debug for Storage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Storage(..)")
    }
}

impl ReadStorage for Storage {
    fn public_key(&self) -> &PublicKey {
        self.signer.public_key()
    }

    fn url(&self) -> git::Url {
        git::Url {
            scheme: git_url::Scheme::File,
            host: None,
            path: self.path.to_string_lossy().to_string().into(),
            ..git::Url::default()
        }
    }

    fn get(&self, id: &Id) -> Result<Option<Project>, Error> {
        // TODO: Don't create a repo here if it doesn't exist?
        // Perhaps for checking we could have a `contains` method?
        let local = self.public_key();
        let repo = self.repository(id)?;

        if let Some(doc) = repo.identity(local)? {
            let remotes = repo.remotes()?.collect::<Result<_, _>>()?;
            let path = repo.path().to_path_buf();

            // TODO: We should check that there is at least one remote, which is
            // the one of the local user, otherwise it means the project is in
            // an corrupted state.

            Ok(Some(Project {
                id: id.clone(),
                doc,
                remotes,
                path,
            }))
        } else {
            Ok(None)
        }
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        self.projects()
    }
}

impl<'r> WriteStorage<'r> for Storage {
    type Repository = Repository;

    fn repository(&self, proj: &Id) -> Result<Self::Repository, Error> {
        Repository::open(self.path.join(proj.to_string()))
    }

    fn sign_refs(&self, repository: &Repository) -> Result<SignedRefs<Verified>, Error> {
        let remote = self.signer.public_key();
        let refs = repository.references(remote)?;
        let signed = refs.signed(self.signer.clone())?;

        signed.save(remote, repository)?;

        Ok(signed)
    }
}

impl Storage {
    pub fn open<P: AsRef<Path>, S: Signer + 'static>(
        path: P,
        signer: S,
    ) -> Result<Self, io::Error> {
        let path = path.as_ref().to_path_buf();

        match fs::create_dir_all(&path) {
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
            Ok(()) => {}
        }

        Ok(Self {
            path,
            signer: Arc::new(signer),
        })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn signer(&self) -> Arc<dyn Signer> {
        self.signer.clone()
    }

    pub fn with_signer(&self, signer: impl Signer + 'static) -> Self {
        Self {
            path: self.path.clone(),
            signer: Arc::new(signer),
        }
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
            let repo = self.repository(&proj)?;

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
    pub(crate) backend: git2::Repository,
    // TODO: Add project id here so we can refer to it
    // in a bunch of places. We could write it to the
    // git config for later.
}

impl Repository {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let backend = match git2::Repository::open_bare(path.as_ref()) {
            Err(e) if git::ext::is_not_found_err(&e) => {
                let backend = git2::Repository::init_opts(
                    path,
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

        Ok(Self { backend })
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

    pub fn identity(&self, remote: &RemoteId) -> Result<Option<identity::Doc>, refs::Error> {
        let oid = if let Some(oid) = self.reference_oid(remote, &RADICLE_ID_REF)? {
            oid
        } else {
            return Ok(None);
        };

        let doc = match self.blob_at(oid, Path::new(&*IDENTITY_PATH)) {
            Err(git::ext::Error::NotFound(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
            Ok(doc) => doc,
        };
        let doc = identity::Doc::from_json(doc.content()).unwrap();

        Ok(Some(doc))
    }
}

impl<'r> ReadRepository<'r> for Repository {
    type Remotes = Box<dyn Iterator<Item = Result<(RemoteId, Remote<Verified>), refs::Error>> + 'r>;

    fn is_empty(&self) -> Result<bool, git2::Error> {
        let some = self.remotes()?.next().is_some();
        Ok(!some)
    }

    fn path(&self) -> &Path {
        self.backend.path()
    }

    fn blob_at<'a>(&'a self, oid: Oid, path: &'a Path) -> Result<git2::Blob<'a>, git::ext::Error> {
        git::ext::Blob::At {
            object: oid.into(),
            path,
        }
        .get(&self.backend)
    }

    fn reference(
        &self,
        remote: &RemoteId,
        name: &git::RefStr,
    ) -> Result<Option<git2::Reference>, git2::Error> {
        let name = format!("refs/remotes/{remote}/{name}");
        self.backend.find_reference(&name).map(Some).or_else(|e| {
            if git::ext::is_not_found_err(&e) {
                Ok(None)
            } else {
                Err(e)
            }
        })
    }

    fn reference_oid(
        &self,
        remote: &RemoteId,
        reference: &git::RefStr,
    ) -> Result<Option<Oid>, git2::Error> {
        let reference = self.reference(remote, reference)?;
        Ok(reference.and_then(|r| r.target().map(|o| o.into())))
    }

    fn remote(&self, remote: &RemoteId) -> Result<Remote<Verified>, refs::Error> {
        let refs = SignedRefs::load(remote, self)?;
        Ok(Remote::new(*remote, refs))
    }

    fn references(&self, remote: &RemoteId) -> Result<Refs, Error> {
        // TODO: Only return known refs, eg. heads/ rad/ tags/ etc..
        let entries = self
            .backend
            .references_glob(format!("refs/remotes/{remote}/*").as_str())?;
        let mut refs = BTreeMap::new();

        for e in entries {
            let e = e?;
            let name = e.name().ok_or(Error::InvalidRef)?;
            let (_, refname) = git::parse_ref::<RemoteId>(name)?;
            let oid = e.target().ok_or(Error::InvalidRef)?;

            refs.insert(refname, oid.into());
        }
        Ok(refs.into())
    }

    fn remotes(&'r self) -> Result<Self::Remotes, git2::Error> {
        let iter = self.backend.references_glob(SIGNATURES_GLOB.as_str())?.map(
            |reference| -> Result<(RemoteId, Remote<Verified>), refs::Error> {
                let r = reference?;
                let name = r.name().ok_or(refs::Error::InvalidRef)?;
                let (id, _) = git::parse_ref::<RemoteId>(name)?;
                let remote = self.remote(&id)?;

                Ok((id, remote))
            },
        );

        Ok(Box::new(iter))
    }
}

impl<'r> WriteRepository<'r> for Repository {
    /// Fetch all remotes of a project from the given URL.
    fn fetch(&mut self, url: &git::Url) -> Result<Vec<RefUpdate>, git2::Error> {
        // TODO: Have function to fetch specific remotes.
        //
        // Repository layout should look like this:
        //
        //   /refs/remotes/<remote>
        //         /heads
        //           /master
        //         /tags
        //         ...
        //
        let url = url.to_string();
        let refs: &[&str] = &["refs/remotes/*:refs/remotes/*"];
        let mut updates = Vec::new();
        let mut callbacks = git2::RemoteCallbacks::new();

        callbacks.update_tips(|name, old, new| {
            if let Ok(name) = git::RefString::try_from(name) {
                updates.push(RefUpdate::from(name, old, new));
            } else {
                log::warn!("Invalid ref `{}` detected; aborting fetch", name);
                return false;
            }
            // Returning `true` ensures the process is not aborted.
            true
        });

        {
            let mut remote = self.backend.remote_anonymous(&url)?;
            let mut opts = git2::FetchOptions::default();
            opts.remote_callbacks(callbacks);

            // TODO: Make sure we verify before pruning, as pruning may get us into
            // a state we can't roll back.
            opts.prune(git2::FetchPrune::On);
            remote.fetch(refs, Some(&mut opts), None)?;
        }

        Ok(updates)
    }

    fn raw(&self) -> &git2::Repository {
        &self.backend
    }
}

impl From<git2::Repository> for Repository {
    fn from(backend: git2::Repository) -> Self {
        Self { backend }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;
    use crate::storage::refs::SIGNATURE_REF;
    use crate::storage::{ReadStorage, WriteRepository};
    use crate::test::arbitrary;
    use crate::test::crypto::MockSigner;
    use crate::test::fixtures;

    #[test]
    fn test_remote_refs() {
        let dir = tempfile::tempdir().unwrap();
        let storage = fixtures::storage(dir.path());
        let inv = storage.inventory().unwrap();
        let proj = inv.first().unwrap();
        let mut refs = git::remote_refs(&git::Url {
            host: Some(dir.path().to_string_lossy().to_string()),
            scheme: git_url::Scheme::File,
            path: format!("/{}", proj).into(),
            ..git::Url::default()
        })
        .unwrap();

        let project = storage.repository(proj).unwrap();
        let remotes = project.remotes().unwrap();

        // Strip the remote refs of sigrefs so we can compare them.
        for remote in refs.values_mut() {
            remote.remove(&*SIGNATURE_REF).unwrap();
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
        let alice = fixtures::storage(tmp.path().join("alice"));
        let bob = Storage::open(tmp.path().join("bob"), MockSigner::default()).unwrap();
        let inventory = alice.inventory().unwrap();
        let proj = inventory.first().unwrap();
        let repo = alice.repository(proj).unwrap();
        let remotes = repo.remotes().unwrap().collect::<Vec<_>>();
        let refname = git::refname!("heads/master");

        // Have Bob fetch Alice's refs.
        let updates = bob
            .repository(proj)
            .unwrap()
            .fetch(&git::Url {
                scheme: git_url::Scheme::File,
                path: alice
                    .path()
                    .join(proj.to_string())
                    .to_string_lossy()
                    .into_owned()
                    .into(),
                ..git::Url::default()
            })
            .unwrap();

        // Four refs are created for each remote.
        assert_eq!(updates.len(), remotes.len() * 4);

        for remote in remotes {
            let (id, _) = remote.unwrap();
            let alice_repo = alice.repository(proj).unwrap();
            let alice_oid = alice_repo.reference(&id, &refname).unwrap().unwrap();

            let bob_repo = bob.repository(proj).unwrap();
            let bob_oid = bob_repo.reference(&id, &refname).unwrap().unwrap();

            assert_eq!(alice_oid.target(), bob_oid.target());
        }
    }

    #[test]
    fn test_sign_refs() {
        let tmp = tempfile::tempdir().unwrap();
        let mut rng = fastrand::Rng::new();
        let signer = MockSigner::new(&mut rng);
        let storage = Storage::open(tmp.path(), signer).unwrap();
        let proj_id = arbitrary::gen::<Id>(1);
        let alice = *storage.public_key();
        let project = storage.repository(&proj_id).unwrap();
        let backend = &project.backend;
        let sig = git2::Signature::now(&alice.to_string(), "anonymous@radicle.xyz").unwrap();
        let head = git::initial_commit(backend, &sig).unwrap();

        let head = git::commit(backend, &head, "Second commit", &alice.to_string()).unwrap();
        backend
            .reference(
                &format!("refs/remotes/{alice}/heads/master"),
                head.id(),
                false,
                "test",
            )
            .unwrap();

        let signed = storage.sign_refs(&project).unwrap();
        let remote = project.remote(&alice).unwrap();
        let mut unsigned = project.references(&alice).unwrap();

        // The signed refs doesn't contain the signature ref itself.
        unsigned.remove(&*SIGNATURE_REF).unwrap();

        assert_eq!(remote.refs, signed);
        assert_eq!(*remote.refs, unsigned);
    }
}
