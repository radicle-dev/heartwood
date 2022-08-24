use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{fmt, fs, io};

use git_ref_format::refspec;
use git_url::Url;
use once_cell::sync::Lazy;
use radicle_git_ext as git_ext;
use serde::{Deserialize, Serialize};

pub use radicle_git_ext::Oid;

use crate::collections::HashMap;
use crate::git;
use crate::identity;
use crate::identity::{ProjId, ProjIdError, UserId};

use super::{
    Error, Inventory, ReadRepository, ReadStorage, Remote, Remotes, Unverified, Verified,
    WriteRepository, WriteStorage,
};

pub static RAD_ROOT_GLOB: Lazy<refspec::PatternString> =
    Lazy::new(|| refspec::pattern!("refs/namespaces/*/refs/rad/root"));
pub static IDENTITY_PATH: Lazy<&Path> = Lazy::new(|| Path::new(".rad/identity.toml"));

pub struct Storage {
    path: PathBuf,
}

impl From<PathBuf> for Storage {
    fn from(path: PathBuf) -> Self {
        Self { path }
    }
}

impl fmt::Debug for Storage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Storage(..)")
    }
}

impl ReadStorage for Storage {
    fn get(&self, _id: &ProjId) -> Result<Option<Remotes<Unverified>>, Error> {
        todo!()
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        let glob: String = RAD_ROOT_GLOB.clone().into();
        let projs = self.projects()?;
        let mut inv = Vec::new();

        for proj in projs {
            let repo = self.repository(&proj)?;
            let remotes = repo
                .remotes()?
                .into_iter()
                .map(|r| (r.id.to_string(), r))
                .collect();

            inv.push((proj, remotes));
        }
        Ok(inv)
    }
}

impl WriteStorage for Storage {
    type Repository = Repository;

    fn repository(&self, proj: &ProjId) -> Result<Self::Repository, Error> {
        Repository::open(self.path.join(proj.to_string()))
    }
}

impl Storage {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref().to_path_buf();

        Self { path }
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn projects(&self) -> Result<Vec<ProjId>, Error> {
        let mut projects = Vec::new();

        for result in fs::read_dir(&self.path)? {
            let path = result?;
            let id = ProjId::try_from(path.file_name())?;

            projects.push(id);
        }
        Ok(projects)
    }

    pub fn create(
        &self,
        repo: &git2::Repository,
        identity: impl Into<identity::Doc>,
    ) -> Result<(ProjId, Oid), Error> {
        let doc = identity.into();
        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(*IDENTITY_PATH)?;
        let id = doc.write(file)?;
        let ref_name = RAD_ROOT_GLOB.replace('*', &id.encode());
        let oid = repo.head()?.target().ok_or(Error::InvalidHead)?;
        let repository = self.repository(&id)?;
        let _reference = repository.backend.reference(&ref_name, oid, false, "")?;

        // TODO: Push project to monorepo.

        Ok((id, oid.into()))
    }
}

pub struct Repository {
    backend: git2::Repository,
}

impl Repository {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let backend = match git2::Repository::open_bare(path.as_ref()) {
            Err(e) if git_ext::is_not_found_err(&e) => {
                let backend = git2::Repository::init_opts(
                    path,
                    git2::RepositoryInitOptions::new()
                        .bare(true)
                        .no_reinit(true)
                        .external_template(false),
                )?;

                Ok(backend)
            }
            Ok(repo) => Ok(repo),
            Err(e) => Err(e),
        }?;

        Ok(Self { backend })
    }

    pub fn find_reference(&self, remote: &UserId, name: &str) -> Result<Oid, Error> {
        let name = format!("refs/namespaces/{}/{}", remote, name);
        let target = self
            .backend
            .find_reference(&name)?
            .target()
            .ok_or(Error::InvalidRef)?;

        Ok(target.into())
    }
}

impl ReadRepository for Repository {
    fn remotes(&self) -> Result<Vec<Remote<Unverified>>, Error> {
        let refs = self.backend.references_glob(RAD_ROOT_GLOB.as_str())?;
        let mut remotes = HashMap::default();

        for r in refs {
            let r = r?;
            let name = r.name().ok_or(Error::InvalidRef)?;
            let (id, refname) = git::parse_ref::<UserId>(name)?;
            let entry = remotes
                .entry(id.clone())
                .or_insert_with(|| Remote::new(id, HashMap::default()));
            let oid = r.target().ok_or(Error::InvalidRef)?;

            entry.refs.insert(refname.to_string(), oid.into());
        }
        Ok(remotes.into_values().collect())
    }
}

impl WriteRepository for Repository {
    /// Fetch all remotes of a project from the given URL.
    fn fetch(&mut self, url: &str) -> Result<(), git2::Error> {
        // TODO: Use `Url` type?
        // TODO: Have function to fetch specific remotes.
        // TODO: Return meaningful info on success.
        //
        // Repository layout should look like this:
        //
        //      /refs/namespaces/<project>
        //              /refs/namespaces/<remote>
        //                    /heads
        //                      /master
        //                    /tags
        //                    ...
        //
        let refs: &[&str] = &[&format!("refs/namespaces/*:refs/namespaces/*")];
        let mut remote = self.backend.remote_anonymous(url)?;
        let mut opts = git2::FetchOptions::default();

        remote.fetch(refs, Some(&mut opts), None)?;

        Ok(())
    }

    fn namespace(&mut self, user: &UserId) -> Result<&mut git2::Repository, git2::Error> {
        let path = self.backend.path();

        self.backend = git2::Repository::open_bare(path)?;
        self.backend.set_namespace(&user.to_string())?;

        Ok(&mut self.backend)
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
    use crate::hash::Digest;
    use crate::identity::ProjId;
    use crate::storage::{ReadStorage, WriteRepository};
    use crate::test::fixtures;

    /// Create an initial empty commit.
    fn initial_commit(repo: &git2::Repository) -> Result<git2::Oid, Error> {
        // First use the config to initialize a commit signature for the user.
        let sig = git2::Signature::now("cloudhead", "cloudhead@radicle.xyz")?;
        // Now let's create an empty tree for this commit.
        let tree_id = repo.index()?.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let oid = repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;

        Ok(oid)
    }

    #[test]
    fn test_ls_remote() {
        crate::test::logger::init(log::Level::Debug);

        let dir = tempfile::tempdir().unwrap();
        let storage = fixtures::storage(dir.path());
        let inv = storage.inventory().unwrap();
        let (proj, _) = inv.first().unwrap();
        let refs = git::list_refs(&format!(
            "file://{}",
            dir.path().join(&proj.to_string()).display(),
        ))
        .unwrap();

        let remotes = storage.repository(&proj).unwrap().remotes().unwrap();

        assert_eq!(refs, remotes);
    }

    #[test]
    fn test_fetch() {
        let path = tempfile::tempdir().unwrap().into_path();
        let alice = fixtures::storage(path.join("alice"));
        let bob = Storage::new(path.join("bob"));
        let inventory = alice.inventory().unwrap();
        let (proj, remotes) = inventory.first().unwrap();
        let refname = "refs/heads/master";

        // Have Bob fetch Alice's refs.
        bob.repository(&proj)
            .unwrap()
            .fetch(&format!(
                "file://{}",
                alice.path().join(&proj.to_string()).display()
            ))
            .unwrap();

        for (_, remote) in remotes {
            let alice_oid = alice
                .repository(&proj)
                .unwrap()
                .find_reference(&remote.id, refname)
                .unwrap();
            let bob_oid = bob
                .repository(&proj)
                .unwrap()
                .find_reference(&remote.id, refname)
                .unwrap();

            assert_eq!(alice_oid, bob_oid);
        }
    }
}
