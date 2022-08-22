use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::{fmt, fs, io, net};

use git_ref_format::refspec;
use once_cell::sync::Lazy;
use radicle_git_ext as git_ext;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use radicle_git_ext::Oid;

use crate::collections::HashMap;
use crate::identity;
use crate::identity::{IdError, ProjId, UserId};

pub static RAD_ID_GLOB: Lazy<refspec::PatternString> =
    Lazy::new(|| refspec::pattern!("refs/namespaces/*/refs/rad/id"));
pub static IDENTITY_PATH: Lazy<&Path> = Lazy::new(|| Path::new(".rad/identity.toml"));

pub type BranchName = String;
pub type Inventory = Vec<(ProjId, Refs)>;

/// Storage error.
#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid git reference")]
    InvalidRef,
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("id: {0}")]
    ProjId(#[from] IdError),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("doc: {0}")]
    Doc(#[from] identity::DocError),
    #[error("invalid repository head")]
    InvalidHead,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct Refs {
    heads: HashMap<BranchName, Oid>,
}

impl From<HashMap<BranchName, Oid>> for Refs {
    fn from(heads: HashMap<BranchName, Oid>) -> Self {
        Self { heads }
    }
}

pub type RemoteId = UserId;
pub type RefName = String;

/// Verified (used as type witness).
pub struct Verified;
/// Unverified (used as type witness).
pub struct Unverified;

/// Project remotes. Tracks the git state of a project.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct Remotes<V>(HashMap<RemoteId, Remote<V>>);

/// A project remote.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct Remote<V> {
    /// Git references published under this remote, and their hashes.
    refs: HashMap<RefName, Oid>,
    /// Whether this remote is of a project delegate.
    delegate: bool,
    /// Whether the remote is verified or not, ie. whether its signed refs were checked.
    verified: PhantomData<V>,
}

pub trait ReadStorage {
    fn get(&self, proj: &ProjId) -> Result<Option<Refs>, Error>;
    fn inventory(&self) -> Result<Inventory, Error>;
}

pub trait WriteStorage {
    /// Fetch a project from a remote peer.
    fn fetch(&mut self, proj: &ProjId, remote: &net::SocketAddr) -> Result<(), Error>;
}

impl<T, S> ReadStorage for T
where
    T: Deref<Target = S>,
    S: ReadStorage,
{
    fn inventory(&self) -> Result<Inventory, Error> {
        self.deref().inventory()
    }

    fn get(&self, proj: &ProjId) -> Result<Option<Refs>, Error> {
        self.deref().get(proj)
    }
}

impl<T, S> WriteStorage for T
where
    T: DerefMut<Target = S>,
    S: WriteStorage,
{
    fn fetch(&mut self, proj: &ProjId, remote: &net::SocketAddr) -> Result<(), Error> {
        self.deref_mut().fetch(proj, remote)
    }
}

pub struct Storage {
    backend: git2::Repository,
}

impl fmt::Debug for Storage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Storage(..)")
    }
}

impl ReadStorage for Storage {
    fn get(&self, _id: &ProjId) -> Result<Option<Refs>, Error> {
        todo!()
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        let glob: String = RAD_ID_GLOB.clone().into();
        let refs = self.backend.references_glob(glob.as_str())?;
        let mut projs = Vec::new();

        for r in refs {
            let r = r?;
            let name = r.name().ok_or(Error::InvalidRef)?;
            let id = ProjId::from_ref(name)?;

            projs.push((id, Refs::default()));
        }
        Ok(projs)
    }
}

impl WriteStorage for Storage {
    fn fetch(&mut self, _id: &ProjId, _remote: &net::SocketAddr) -> Result<(), Error> {
        todo!()
    }
}

impl Storage {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, git2::Error> {
        let path = path.as_ref();
        let backend = match git2::Repository::open_bare(path) {
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

    pub fn create(
        &self,
        repo: &git2::Repository,
        identity: impl Into<identity::Doc>,
    ) -> Result<(ProjId, git2::Reference), Error> {
        let doc = identity.into();
        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(*IDENTITY_PATH)?;
        let id = doc.write(file)?;
        let ref_name = RAD_ID_GLOB.replace('*', &id.encode());
        let oid = repo.head()?.target().ok_or(Error::InvalidHead)?;
        let reference = self.backend.reference(&ref_name, oid, false, "")?;

        // TODO: Push project to monorepo.

        Ok((id, reference))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_storage() {}
}
