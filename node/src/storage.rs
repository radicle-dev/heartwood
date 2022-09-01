pub mod git;

use std::collections::hash_map;
use std::io;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::path::Path;

use git_url::Url;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use radicle_git_ext::Oid;

use crate::collections::HashMap;
use crate::git::RefError;
use crate::identity;
use crate::identity::{ProjId, ProjIdError, UserId};

pub static IDENTITY_PATH: Lazy<&Path> = Lazy::new(|| Path::new(".rad/identity.toml"));

pub type BranchName = String;
pub type Inventory = Vec<ProjId>;

/// Storage error.
#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid git reference")]
    InvalidRef,
    #[error("git reference error: {0}")]
    Ref(#[from] RefError),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("id: {0}")]
    ProjId(#[from] ProjIdError),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("doc: {0}")]
    Doc(#[from] identity::DocError),
    #[error("invalid repository head")]
    InvalidHead,
}

pub type Refs = HashMap<RefName, Oid>;
pub type RemoteId = UserId;
pub type RefName = String;

/// Verified (used as type witness).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verified;
/// Unverified (used as type witness).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unverified;

/// Project remotes. Tracks the git state of a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remotes<V>(HashMap<RemoteId, Remote<V>>);

impl Remotes<Unverified> {
    pub fn new(remotes: HashMap<RemoteId, Remote<Unverified>>) -> Self {
        Self(remotes)
    }
}

impl Default for Remotes<Unverified> {
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

#[allow(clippy::from_over_into)]
impl Into<HashMap<String, Remote<Unverified>>> for Remotes<Unverified> {
    fn into(self) -> HashMap<String, Remote<Unverified>> {
        let mut remotes = HashMap::with_hasher(fastrand::Rng::new().into());

        for (k, v) in self.0 {
            remotes.insert(k.to_string(), v);
        }
        remotes
    }
}

/// A project remote.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Remote<V> {
    /// ID of remote.
    pub id: UserId,
    /// Git references published under this remote, and their hashes.
    pub refs: HashMap<RefName, Oid>,
    /// Whether this remote is of a project delegate.
    pub delegate: bool,
    /// Whether the remote is verified or not, ie. whether its signed refs were checked.
    #[serde(skip)]
    verified: PhantomData<V>,
}

impl Remote<Unverified> {
    pub fn new(id: UserId, refs: HashMap<RefName, Oid>) -> Self {
        Self {
            id,
            refs,
            delegate: false,
            verified: PhantomData,
        }
    }
}

pub trait ReadStorage {
    fn user_id(&self) -> &UserId;
    fn url(&self) -> Url;
    fn get(&self, proj: &ProjId) -> Result<Option<Remotes<Unverified>>, Error>;
    fn inventory(&self) -> Result<Inventory, Error>;
}

pub trait WriteStorage: ReadStorage {
    type Repository: WriteRepository;

    fn repository(&self, proj: &ProjId) -> Result<Self::Repository, Error>;
}

pub trait ReadRepository {
    fn path(&self) -> &Path;
    fn remote(&self, user: &UserId) -> Result<Remote<Unverified>, Error>;
    fn remotes(&self) -> Result<Remotes<Unverified>, Error>;
}

pub trait WriteRepository: ReadRepository {
    fn fetch(&mut self, url: &Url) -> Result<(), git2::Error>;
    fn namespace(&mut self, user: &UserId) -> Result<&mut git2::Repository, git2::Error>;
}

impl<T, S> ReadStorage for T
where
    T: Deref<Target = S>,
    S: ReadStorage + 'static,
{
    fn user_id(&self) -> &UserId {
        self.deref().user_id()
    }

    fn url(&self) -> Url {
        self.deref().url()
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        self.deref().inventory()
    }

    fn get(&self, proj: &ProjId) -> Result<Option<Remotes<Unverified>>, Error> {
        self.deref().get(proj)
    }
}

impl<T, S> WriteStorage for T
where
    T: DerefMut<Target = S>,
    S: WriteStorage + 'static,
{
    type Repository = S::Repository;

    fn repository(&self, proj: &ProjId) -> Result<Self::Repository, Error> {
        self.deref().repository(proj)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_storage() {}
}
